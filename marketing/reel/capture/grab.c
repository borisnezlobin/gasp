/*
 * grab: a screen recorder for the capture display that keeps up at 60 fps.
 *
 * ffmpeg's x11grab can read a 2560x1600 screen 60 times a second here, but
 * nothing can encode that many pixels on this machine while the app also
 * renders in software, and even grabbing every tick slows the app's own
 * drawing through the X server. Most ticks don't change a pixel, though. So
 * this runs a fixed 60 Hz clock, and on each tick grabs the screen (MIT-SHM)
 * only if the X server reported damage since the last grab (or the pointer
 * moved, with --cursor), and stores only frames that changed, LZ4-compressed,
 * with the tick they were grabbed on. `grab decode` turns that back into a
 * constant 60 fps stream of raw frames for ffmpeg, repeating a frame on the
 * ticks where the screen didn't change. Nothing is interpolated or edited.
 *
 *   grab record <out.grab> [--cursor]     (stops on SIGINT or SIGTERM)
 *   grab decode <in.grab> | ffmpeg -f rawvideo -pix_fmt bgr0 -s WxH -r 60 -i - ...
 *   grab unique <in.grab> <index.txt> | ffmpeg -f rawvideo -pix_fmt bgr0 -s WxH -i - frames/%05d.png
 *
 * Build: cc -O2 -o grab grab.c -lX11 -lXext /usr/lib/x86_64-linux-gnu/libXfixes.so.3 \
 *        /usr/lib/x86_64-linux-gnu/libXdamage.so.1 /usr/lib/x86_64-linux-gnu/liblz4.so.1
 * (Debian ships the Xfixes, Xdamage and LZ4 libraries without headers here, so the
 * few functions used are declared below.)
 */
#define _GNU_SOURCE
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <X11/extensions/XShm.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ipc.h>
#include <sys/shm.h>
#include <time.h>

int LZ4_compressBound(int inputSize);
int LZ4_compress_fast(const char *src, char *dst, int srcSize, int dstCapacity, int acceleration);
int LZ4_decompress_safe(const char *src, char *dst, int compressedSize, int dstCapacity);

typedef struct {
    short x, y;
    unsigned short width, height;
    unsigned short xhot, yhot;
    unsigned long cursor_serial;
    unsigned long *pixels;
    Atom atom;
    const char *name;
} XFixesCursorImage;
XFixesCursorImage *XFixesGetCursorImage(Display *dpy);
int XFixesQueryExtension(Display *dpy, int *event_base, int *error_base);

typedef XID Damage;
int XDamageQueryExtension(Display *dpy, int *event_base, int *error_base);
Damage XDamageCreate(Display *dpy, Drawable drawable, int level);
void XDamageSubtract(Display *dpy, Damage damage, XID repair, XID parts);
#define XDamageReportNonEmpty 3

#define FPS 60
static volatile sig_atomic_t stopping = 0;
static void on_signal(int sig) { (void)sig; stopping = 1; }

static double now_real(void) {
    struct timespec ts;
    clock_gettime(CLOCK_REALTIME, &ts);
    return ts.tv_sec + ts.tv_nsec / 1e9;
}

static void draw_cursor(Display *dpy, uint32_t *frame, int width, int height) {
    XFixesCursorImage *cursor = XFixesGetCursorImage(dpy);
    if (!cursor) return;
    int left = cursor->x - cursor->xhot, top = cursor->y - cursor->yhot;
    for (int y = 0; y < cursor->height; y++) {
        int sy = top + y;
        if (sy < 0 || sy >= height) continue;
        for (int x = 0; x < cursor->width; x++) {
            int sx = left + x;
            if (sx < 0 || sx >= width) continue;
            uint32_t argb = (uint32_t)cursor->pixels[y * cursor->width + x];
            uint32_t a = argb >> 24;
            if (!a) continue;
            uint32_t *dst = &frame[sy * width + sx];
            uint32_t out = 0;
            for (int shift = 0; shift < 24; shift += 8) {
                /* Cursor pixels are premultiplied. */
                uint32_t s = (argb >> shift) & 0xff, d = (*dst >> shift) & 0xff;
                uint32_t v = s + d * (255 - a) / 255;
                out |= (v > 255 ? 255 : v) << shift;
            }
            *dst = out;
        }
    }
    XFree(cursor);
}

static int every_tick = 0;

static int record(const char *path, int with_cursor) {
    Display *dpy = XOpenDisplay(NULL);
    if (!dpy) { fprintf(stderr, "no display\n"); return 1; }
    int screen = DefaultScreen(dpy);
    Window root = RootWindow(dpy, screen);
    int width = DisplayWidth(dpy, screen), height = DisplayHeight(dpy, screen);
    if (with_cursor) { int eb, erb; XFixesQueryExtension(dpy, &eb, &erb); }
    int damage_event, damage_error;
    if (!XDamageQueryExtension(dpy, &damage_event, &damage_error)) { fprintf(stderr, "no DAMAGE\n"); return 1; }
    Damage damage = XDamageCreate(dpy, root, XDamageReportNonEmpty);
    int cursor_x = -1, cursor_y = -1;
    XShmSegmentInfo shm;
    XImage *image = XShmCreateImage(dpy, DefaultVisual(dpy, screen), DefaultDepth(dpy, screen), ZPixmap,
                                    NULL, &shm, width, height);
    shm.shmid = shmget(IPC_PRIVATE, image->bytes_per_line * image->height, IPC_CREAT | 0600);
    shm.shmaddr = image->data = shmat(shm.shmid, NULL, 0);
    shm.readOnly = False;
    XShmAttach(dpy, &shm);
    XSync(dpy, False);
    shmctl(shm.shmid, IPC_RMID, NULL);

    size_t frame_bytes = (size_t)width * height * 4;
    uint32_t *current = malloc(frame_bytes), *previous = calloc(1, frame_bytes);
    int bound = LZ4_compressBound((int)frame_bytes);
    char *packed = malloc(bound);
    FILE *out = fopen(path, "wb");
    static char buffer[1 << 24];
    setvbuf(out, buffer, _IOFBF, sizeof buffer);
    fwrite("GRAB1", 1, 5, out);
    int32_t dims[2] = {width, height};
    fwrite(dims, sizeof dims, 1, out);

    signal(SIGINT, on_signal);
    signal(SIGTERM, on_signal);
    struct timespec next;
    clock_gettime(CLOCK_MONOTONIC, &next);
    double start = now_real();
    fwrite(&start, sizeof start, 1, out);
    fflush(out);
    fprintf(stderr, "start %.6f %dx%d\n", start, width, height);
    int64_t tick = 0, stored = 0, missed = 0;
    int first = 1;
    const long period = 1000000000L / FPS;
    int64_t grabs = 0;
    while (!stopping) {
        int damaged = first || every_tick;
        while (XPending(dpy)) {
            XEvent event;
            XNextEvent(dpy, &event);
            if (event.type == damage_event + 0) damaged = 1;
        }
        if (with_cursor) {
            Window r, c; int rx, ry, wx, wy; unsigned int mask;
            XQueryPointer(dpy, root, &r, &c, &rx, &ry, &wx, &wy, &mask);
            if (rx != cursor_x || ry != cursor_y) { damaged = 1; cursor_x = rx; cursor_y = ry; }
        }
        int32_t size = 0;
        if (damaged) {
            XDamageSubtract(dpy, damage, None, None);
            XShmGetImage(dpy, root, image, 0, 0, AllPlanes);
            memcpy(current, image->data, frame_bytes);
            if (with_cursor) draw_cursor(dpy, current, width, height);
            grabs++;
        }
        if (damaged && (first || memcmp(current, previous, frame_bytes) != 0)) {
            size = LZ4_compress_fast((const char *)current, packed, (int)frame_bytes, bound, 1);
            uint32_t *swap = previous; previous = current; current = swap;
            stored++;
            first = 0;
        }
        fwrite(&tick, sizeof tick, 1, out);
        fwrite(&size, sizeof size, 1, out);
        if (size) fwrite(packed, 1, size, out);
        /* The next tick on the fixed clock; ticks already past are missed. */
        tick++;
        next.tv_nsec += period;
        while (next.tv_nsec >= 1000000000L) { next.tv_nsec -= 1000000000L; next.tv_sec++; }
        struct timespec mono;
        clock_gettime(CLOCK_MONOTONIC, &mono);
        while (mono.tv_sec > next.tv_sec || (mono.tv_sec == next.tv_sec && mono.tv_nsec > next.tv_nsec)) {
            tick++;
            missed++;
            next.tv_nsec += period;
            while (next.tv_nsec >= 1000000000L) { next.tv_nsec -= 1000000000L; next.tv_sec++; }
        }
        clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, &next, NULL);
    }
    fclose(out);
    fprintf(stderr, "grabs %lld\n", (long long)grabs);
    fprintf(stderr, "ticks %lld stored %lld missed %lld\n", (long long)tick, (long long)stored, (long long)missed);
    return 0;
}

/* Writes only the frames that changed to stdout, and to `index_path` a
 * line per tick: the tick and the number of the changed frame it shows. */
static int unique(const char *path, const char *index_path) {
    FILE *in = fopen(path, "rb");
    if (!in) { perror(path); return 1; }
    char magic[5];
    int32_t dims[2];
    double start;
    if (fread(magic, 1, 5, in) != 5 || memcmp(magic, "GRAB1", 5) || fread(dims, sizeof dims, 1, in) != 1 ||
        fread(&start, sizeof start, 1, in) != 1) {
        fprintf(stderr, "not a grab file\n");
        return 1;
    }
    FILE *index = fopen(index_path, "w");
    fprintf(index, "start %.6f %d %d\n", start, dims[0], dims[1]);
    size_t frame_bytes = (size_t)dims[0] * dims[1] * 4;
    char *frame = calloc(1, frame_bytes), *packed = malloc(LZ4_compressBound((int)frame_bytes));
    int64_t tick, next_tick = 0;
    int32_t size;
    long frames = -1;
    while (fread(&tick, sizeof tick, 1, in) == 1 && fread(&size, sizeof size, 1, in) == 1) {
        /* Ticks the recorder missed show the last frame it saw. */
        for (; next_tick < tick; next_tick++) fprintf(index, "%lld %ld\n", (long long)next_tick, frames);
        if (size) {
            if (fread(packed, 1, size, in) != (size_t)size) break;
            LZ4_decompress_safe(packed, frame, size, (int)frame_bytes);
            fwrite(frame, 1, frame_bytes, stdout);
            frames++;
        }
        fprintf(index, "%lld %ld\n", (long long)tick, frames);
        next_tick = tick + 1;
    }
    fclose(index);
    fprintf(stderr, "unique frames %ld over %lld ticks\n", frames + 1, (long long)next_tick);
    return 0;
}

static int decode(const char *path) {
    FILE *in = fopen(path, "rb");
    if (!in) { perror(path); return 1; }
    char magic[5];
    int32_t dims[2];
    double start;
    if (fread(magic, 1, 5, in) != 5 || memcmp(magic, "GRAB1", 5) || fread(dims, sizeof dims, 1, in) != 1 ||
        fread(&start, sizeof start, 1, in) != 1) {
        fprintf(stderr, "not a grab file\n");
        return 1;
    }
    size_t frame_bytes = (size_t)dims[0] * dims[1] * 4;
    char *frame = calloc(1, frame_bytes), *packed = malloc(LZ4_compressBound((int)frame_bytes));
    int64_t tick, written = 0, repeated = 0;
    int32_t size;
    while (fread(&tick, sizeof tick, 1, in) == 1 && fread(&size, sizeof size, 1, in) == 1) {
        if (size) {
            if (fread(packed, 1, size, in) != (size_t)size) break;
            LZ4_decompress_safe(packed, frame, size, (int)frame_bytes);
        }
        /* Ticks the recorder missed show the last frame it saw. */
        while (written < tick) { fwrite(frame, 1, frame_bytes, stdout); written++; repeated++; }
        fwrite(frame, 1, frame_bytes, stdout);
        written++;
    }
    fprintf(stderr, "start %.6f size %dx%d frames %lld missed-ticks-filled %lld\n", start, dims[0], dims[1],
            (long long)written, (long long)repeated);
    return 0;
}

int main(int argc, char **argv) {
    if (argc >= 3 && !strcmp(argv[1], "record")) {
        int cursor = 0;
        for (int i = 3; i < argc; i++) {
            if (!strcmp(argv[i], "--cursor")) cursor = 1;
            if (!strcmp(argv[i], "--every-tick")) every_tick = 1;
        }
        return record(argv[2], cursor);
    }
    if (argc >= 3 && !strcmp(argv[1], "decode")) return decode(argv[2]);
    if (argc >= 4 && !strcmp(argv[1], "unique")) return unique(argv[2], argv[3]);
    fprintf(stderr, "usage: grab record <out.grab> [--cursor] | grab decode <in.grab> | grab unique <in.grab> <index>\n");
    return 2;
}
