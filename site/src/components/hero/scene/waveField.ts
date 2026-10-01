/** A water surface as a height field on a grid, stepped with the wave
    equation: each cell is pulled towards its neighbours' average, so a push
    anywhere spreads outward as rings and fades. Heights are in metres. */
export class WaveField {
  readonly columns: number;
  readonly rows: number;
  private readonly minX: number;
  private readonly minZ: number;
  private readonly cell: number;
  private current: Float32Array;
  private previous: Float32Array;
  private next: Float32Array;

  constructor(bounds: { minX: number; maxX: number; minZ: number; maxZ: number }, cell: number) {
    this.cell = cell;
    this.minX = bounds.minX;
    this.minZ = bounds.minZ;
    this.columns = Math.ceil((bounds.maxX - bounds.minX) / cell) + 1;
    this.rows = Math.ceil((bounds.maxZ - bounds.minZ) / cell) + 1;
    const size = this.columns * this.rows;
    this.current = new Float32Array(size);
    this.previous = new Float32Array(size);
    this.next = new Float32Array(size);
  }

  /** Raises the water by `amount` metres in a soft disc of `radius`. */
  push(x: number, z: number, amount: number, radius: number) {
    const reach = Math.ceil(radius / this.cell) + 1;
    const centreColumn = (x - this.minX) / this.cell;
    const centreRow = (z - this.minZ) / this.cell;
    const firstColumn = Math.max(1, Math.floor(centreColumn - reach));
    const lastColumn = Math.min(this.columns - 2, Math.ceil(centreColumn + reach));
    const firstRow = Math.max(1, Math.floor(centreRow - reach));
    const lastRow = Math.min(this.rows - 2, Math.ceil(centreRow + reach));
    for (let row = firstRow; row <= lastRow; row++) {
      for (let column = firstColumn; column <= lastColumn; column++) {
        const distance = Math.hypot(column - centreColumn, row - centreRow) * this.cell;
        if (distance > radius) continue;
        const falloff = 0.5 + 0.5 * Math.cos((Math.PI * distance) / radius);
        this.current[row * this.columns + column] += amount * falloff;
      }
    }
  }

  /** One step of `dt` seconds. `speed` is how fast rings travel. */
  step(dt: number, speed = 9, damping = 0.985) {
    const pull = Math.min(0.49, (speed * dt) / this.cell) ** 2;
    const { columns, rows, current, previous, next } = this;
    for (let row = 1; row < rows - 1; row++) {
      for (let column = 1; column < columns - 1; column++) {
        const index = row * columns + column;
        const around = current[index - 1] + current[index + 1] + current[index - columns] + current[index + columns];
        const value = 2 * current[index] - previous[index] + pull * (around - 4 * current[index]);
        next[index] = value * damping;
      }
    }
    this.previous = current;
    this.current = next;
    this.next = previous;
  }

  /** The height at a point, blended from the four cells around it. */
  heightAt(x: number, z: number): number {
    const column = Math.min(this.columns - 2, Math.max(0, (x - this.minX) / this.cell));
    const row = Math.min(this.rows - 2, Math.max(0, (z - this.minZ) / this.cell));
    const left = Math.floor(column);
    const top = Math.floor(row);
    const across = column - left;
    const down = row - top;
    const at = (c: number, r: number) => this.current[r * this.columns + c];
    const upper = at(left, top) * (1 - across) + at(left + 1, top) * across;
    const lower = at(left, top + 1) * (1 - across) + at(left + 1, top + 1) * across;
    return upper * (1 - down) + lower * down;
  }

  /** The surface's slope along x and z at a point. */
  slopeAt(x: number, z: number): { x: number; z: number } {
    const step = this.cell;
    return {
      x: (this.heightAt(x + step, z) - this.heightAt(x - step, z)) / (2 * step),
      z: (this.heightAt(x, z + step) - this.heightAt(x, z - step)) / (2 * step),
    };
  }
}
