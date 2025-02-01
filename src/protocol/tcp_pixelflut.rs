use std::io;

use monoio::{
    buf::IoBuf,
    io::{AsyncReadRent, AsyncWriteRent},
    net::TcpStream,
    time::Instant,
};

use crate::core::{
    image::{Coord, PixelflutImage, RGBAPixel, Timestamp},
    state::PixelflutIOWorkerState,
};

pub struct PixelflutClient {
    stream: TcpStream,
    worker: &'static PixelflutIOWorkerState,

    base_x: Coord,
    base_y: Coord,
}

impl PixelflutClient {
    pub fn new(stream: TcpStream, worker: &'static PixelflutIOWorkerState) -> PixelflutClient {
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
        Some(PixelflutCommand::Help)
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
                self.respond("This is pixelflut (Rust-version) by Nikita\r\n")
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
                let image = unsafe { self.worker.my_present_queue.producer_buffer() };
                let Some((abs_x, abs_y)) = self.boundscheck(x, y, image) else {
                    self.respond_error("error: pixel out of bounds").await?;
                    return Ok(());
                };

                image.set_pixel(
                    abs_x,
                    abs_y,
                    pixel,
                    Instant::now()
                        .duration_since(self.worker.global_config.start_time)
                        // FIXME: this is unsound/not exactly elegant (maybe get the timer only at the start of request inbound?)
                        .as_nanos() as Timestamp,
                );
            }
            PixelflutCommand::Offset { x, y } => {
                self.base_x = x;
                self.base_y = y;
            }
        })
    }

    pub async fn dispatch_line(&mut self, line: &[u8]) -> io::Result<()> {
        let Some(cmd) = parse_pixelflut_request(line) else {
            self.respond_error("error: syntax error or unknown command\r\n")
                .await?;
            return Ok(());
        };

        self.execute_command(cmd).await?;

        Ok(())
    }
}

pub async fn io_task(
    mut client: PixelflutClient,
) -> io::Result<()> {
    let mut rxbuf: Vec<u8> = Vec::with_capacity(4096);
    let mut res;
    loop {
        (res, rxbuf) = client.stream.read(rxbuf).await;
        res?;

        let mut last_newline = 0;
        for newline in rxbuf
            .iter()
            .enumerate()
            .filter(|(_i, &b)| b == b'\n')
            .map(|(i, _b)| i)
        {
            let mut newline2 = newline;
            if newline2 > 0 && rxbuf[newline2 - 1] == b'\r' {
                newline2 -= 1;
            }
            let line = &rxbuf[last_newline..newline2];
            client.dispatch_line(line).await?;

            last_newline = newline + 1;
        }
        rxbuf.drain(..last_newline);

        // Maximum bytes/line
        if rxbuf.len() > 128 {
            // FIXME: maybe re-sync until \r\n? \n?
            client.respond_error("error: request line too long (discarded)").await?;
            rxbuf.clear();
        }

        // FIXME: this is hacky and misplaced, but it seems to work?
        // FIXME: this should be done once every *1ms*, not this often
        client.worker.my_present_queue.swap_present_side();
    }
}