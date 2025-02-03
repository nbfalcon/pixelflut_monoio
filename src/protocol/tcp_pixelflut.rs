use core::str;
use std::io;

use arrayvec::ArrayVec;
use monoio::{
    buf::IoBuf,
    io::{AsyncReadRent, AsyncWriteRent},
    net::TcpStream,
};

use crate::core::{
    image::{Coord, PixelflutImage, RGBAPixel},
    state::PixelflutThreadState,
};

pub struct PixelflutClient {
    stream: TcpStream,
    worker: &'static PixelflutThreadState,

    base_x: Coord,
    base_y: Coord,
}

impl PixelflutClient {
    pub fn new(stream: TcpStream, worker: &'static PixelflutThreadState) -> PixelflutClient {
        Self {
            stream,
            worker,
            base_x: 0,
            base_y: 0,
        }
    }
}

fn parse_hex1(hx_char: u8) -> Option<u8> {
    if hx_char >= b'0' && hx_char <= b'9' {
        Some(hx_char - b'0')
    } else if hx_char >= b'A' && hx_char <= b'F' {
        Some(hx_char - b'A' + 10u8)
    } else if hx_char >= b'a' && hx_char <= b'f' {
        Some(hx_char - b'a' + 10u8)
    } else {
        None
    }
}

fn parse_hex2(hx_hi: u8, hx_lo: u8) -> Option<u8> {
    let hi = parse_hex1(hx_hi)?;
    let lo = parse_hex1(hx_lo)?;
    Some((hi << 4) | lo)
}

fn parse_rgba(w_rgba: &[u8]) -> Option<RGBAPixel> {
    if w_rgba.len() == 6 {
        // RGB, no opacity
        let r = parse_hex2(w_rgba[0], w_rgba[1])?;
        let g = parse_hex2(w_rgba[2], w_rgba[3])?;
        let b = parse_hex2(w_rgba[4], w_rgba[5])?;
        Some(RGBAPixel::new_rgb(r, g, b))
    } else if w_rgba.len() == 8 {
        let r = parse_hex2(w_rgba[0], w_rgba[1])?;
        let g = parse_hex2(w_rgba[2], w_rgba[3])?;
        let b = parse_hex2(w_rgba[4], w_rgba[5])?;
        let a = parse_hex2(w_rgba[6], w_rgba[7])?;
        Some(RGBAPixel::new_rgba(r, g, b, a))
    } else if w_rgba.len() == 3 {
        let r = parse_hex1(w_rgba[0])?;
        let g = parse_hex1(w_rgba[1])?;
        let b = parse_hex1(w_rgba[2])?;
        Some(RGBAPixel::new_rgb(r, g, b))
    } else {
        None
    }
}

fn break_whitespace(s: &[u8]) -> impl Iterator<Item = &[u8]> {
    s.split(|c| c.is_ascii_whitespace())
        .filter(|&s| !s.is_empty())
}

fn atoi_coord(decimal: &[u8]) -> Option<Coord> {
    if decimal.is_empty() {
        return None;
    }

    let mut result: Coord = 0;
    for &digit in decimal.iter() {
        if digit >= b'0' && digit <= b'9' {
            let dec = digit - b'0';
            result = result.checked_mul(10)?.checked_add(dec as Coord)?;
        } else {
            return None;
        }
    }

    Some(result)
}

pub enum PixelflutCommand {
    Help,
    Size,
    SetPixel {
        x: Coord,
        y: Coord,
        pixel: RGBAPixel,
    },
    Offset {
        x: Coord,
        y: Coord,
    },
}

fn parse_pixelflut_request(line: &[u8]) -> Option<PixelflutCommand> {
    let mut split = break_whitespace(line);

    let subcommand = split.next()?;
    if subcommand == b"PX" {
        let w_x = split.next()?;
        let w_y = split.next()?;
        let w_rgba = split.next()?;

        let r_rgba = parse_rgba(w_rgba)?;
        let r_x = atoi_coord(w_x)?;
        let r_y = atoi_coord(w_y)?;

        Some(PixelflutCommand::SetPixel {
            x: r_x,
            y: r_y,
            pixel: r_rgba,
        })
    } else if subcommand == b"SIZE" {
        Some(PixelflutCommand::Size)
    } else if subcommand == b"HELP" {
        Some(PixelflutCommand::Help)
    } else if subcommand == b"OFFSET" {
        let w_x = split.next()?;
        let w_y = split.next()?;
        let r_x = atoi_coord(w_x)?;
        let r_y = atoi_coord(w_y)?;
        Some(PixelflutCommand::Offset { x: r_x, y: r_y })
    } else {
        None
    }
}

const HELP_TEXT: &str =
    "Pixelflut Server by Nikita Bloshchanevich (https://github.com/nbfalcon/pixelflut_monoio)

Accepted Commands:
- OFFSET X Y: configure the offset for all subsequent PX commands (X and Y are added to X Y from PX)
- PX X Y <hex-color code: RGB | RRGGBB | RRGGBBAA>: set pixel at X, Y to color
- SIZE: return the SIZE of the board (response is a line SIZE <width> <height>)

All numbers are in decimal (except color codes).

Examples:
PX 10 10 FFF
PX 10 11 ffaa00
PX 10 12 ffaa00ff\r\n";
// USE \r\n to terminate the message. This is a bit hacky, but this way, the client can always just assume reading until \r\n for respones.

impl PixelflutClient {
    async fn respond<T: IoBuf>(&mut self, s: T) -> io::Result<()> {
        self.stream.write(s).await.0?;
        Ok(())
    }

    async fn respond_error<T: IoBuf>(&mut self, s: T) -> io::Result<()> {
        self.respond(s).await
    }

    fn boundscheck(&self, x: u32, y: u32, image: &PixelflutImage) -> Option<(Coord, Coord)> {
        if let Some(real_x) = x.checked_add(self.base_x) {
            if let Some(real_y) = y.checked_add(self.base_y) {
                if image.bounds_check(real_x, real_y) {
                    return Some((real_x, real_y));
                }
            }
        }
        None
    }

    pub async fn execute_command(&mut self, cmd: PixelflutCommand) -> Result<(), io::Error> {
        Ok(match cmd {
            PixelflutCommand::Help => {
                self.respond(HELP_TEXT)
                    .await?;
            }
            PixelflutCommand::Size => {
                let w = self.worker.global_config.width;
                let h = self.worker.global_config.height;
                self.stream
                    .write(format!("SIZE {w} {h}\r\n").into_bytes())
                    .await
                    .0?;
            }
            PixelflutCommand::SetPixel { x, y, pixel } => {
                let image = &self.worker.global_state.image;
                let Some((abs_x, abs_y)) = self.boundscheck(x, y, image) else {
                    self.respond_error("error: pixel out of bounds").await?;
                    return Ok(());
                };

                // FIXME: blend in CAS here
                image.set_pixel(abs_x, abs_y, pixel);
            }
            PixelflutCommand::Offset { x, y } => {
                self.base_x = x;
                self.base_y = y;
            } // FIXME: better error messages
              // 1. Handle the case of unknown command better
              // 2. "Expect no more arguments"
              // 3. A Use result instead of hacking on top Option<>?
        })
    }

    pub async fn dispatch_line(&mut self, line: &[u8]) -> io::Result<()> {
        let Some(cmd) = parse_pixelflut_request(line) else {
            let line_s = str::from_utf8(line).unwrap(); // FIXME: this is wrong (maybe we can do utf8 conversion that doesn't panic attacker-controlled?)
            let errmsg = format!("error: syntax error or unknown command '{line_s}'\r\n");
            eprintln!("{errmsg}");
            self.respond_error(errmsg.into_bytes()).await?;
            return Ok(());
        };

        self.execute_command(cmd).await?;

        Ok(())
    }
}

pub async fn io_task(mut client: PixelflutClient) -> io::Result<()> {
    let mut linebuf = ArrayVec::<u8, 128>::new();
    let mut rxbuf: Vec<u8> = Vec::with_capacity(4096);
    loop {
        let res;
        (res, rxbuf) = client.stream.read(rxbuf).await;
        if res? == 0 {
            break; // Handle EOF: https://github.com/bytedance/monoio/blob/master/examples/echo.rs
        }

        let mut split = rxbuf
            .split(|&c| c == b'\n')
            .map(|mut l| {
                // Remove carriage return
                if let Some((b'\r', rest)) = l.split_last() {
                    l = rest;
                }
                l
            })
            .peekable();

        let first_segment = split.next().unwrap();
        if linebuf.try_extend_from_slice(first_segment).is_ok() {
            client.dispatch_line(linebuf.as_slice()).await?;
            linebuf.clear();
        } else {
            client
                .respond_error("error: line too long (discarding)")
                .await?;
        }
        while let Some(line) = split.next() {
            if line.len() > linebuf.capacity() {
                client
                    .respond_error("error: line too long (discarding)")
                    .await?;
                continue;
            }

            if split.peek().is_some() {
                client.dispatch_line(line).await?;
            } else {
                // Last element
                linebuf.clear();
                linebuf.try_extend_from_slice(line).unwrap();
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_pixelflut_request, parse_rgba};

    #[test]
    fn test_parsers() {
        parse_rgba(b"ffff00").unwrap();
        parse_pixelflut_request(b"PX 24 50 ffff00").unwrap();
    }
}
