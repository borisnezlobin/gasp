//! Moves the X11 pointer from a separate connection; see
//! `app::start_x11_wake`.

use std::error::Error;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt;
use x11rb::wrapper::ConnectionExt as _;

/// Moves the pointer to the middle of the screen, in two steps so the
/// window under it gets motion even if the pointer started there.
pub fn move_pointer_to_screen_center() -> Result<(), Box<dyn Error>> {
    let (connection, screen_index) = x11rb::connect(None)?;
    let screen = &connection.setup().roots[screen_index];
    let center_x = (screen.width_in_pixels / 2) as i16;
    let center_y = (screen.height_in_pixels / 2) as i16;
    for offset in [0, 1] {
        connection.warp_pointer(
            x11rb::NONE,
            screen.root,
            0,
            0,
            0,
            0,
            center_x + offset,
            center_y + offset,
        )?;
        connection.flush()?;
    }
    connection.sync()?;
    Ok(())
}
