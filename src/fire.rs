use crossterm::{
    cursor,
    style::{Color, Print, SetBackgroundColor},
    terminal::{self, Clear, ClearType},
    ExecutableCommand,
};
use rand::Rng;
use std::io::{stdout, Write};
use std::thread::sleep;
use std::time::Duration;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

pub fn display_fire(stop_flag: Arc<AtomicBool>) -> crossterm::Result<()> {
    let mut stdout = stdout();
    let (width, height) = terminal::size()?;
    let width = width as usize;
    let height = height as usize;

    let mut rng = rand::thread_rng();
    let mut fire = vec![vec![0u8; width]; height];

    for x in 0..width {
        fire[height - 1][x] = rng.gen_range(128..=255);
    }

    while !stop_flag.load(Ordering::SeqCst) {
        for y in (1..height).rev() {
            for x in 0..width {
                let decay = rng.gen_range(0..3);
                let below = fire[y][x] as usize;
                let new_value = if below > decay {
                    below - decay
                } else {
                    0
                };
                fire[y - 1][x] = new_value as u8;
            }
        }

        stdout.execute(Clear(ClearType::All))?;
        for y in 0..height {
            for x in 0..width {
                let intensity = fire[y][x];
                let color = match intensity {
                    0..=63 => Color::Black,
                    64..=127 => Color::DarkRed,
                    128..=191 => Color::Red,
                    _ => Color::Yellow,
                };
                stdout
                    .execute(SetBackgroundColor(color))?
                    .execute(Print(" "))?;
            }
            stdout.execute(Print("\n"))?;
        }

        let burning_text = "BURNING";
        let text_len = burning_text.len();
        let start_pos = (width / 2).saturating_sub(text_len / 2);
        stdout
            .execute(cursor::MoveTo(start_pos as u16, height as u16 - 1))?
            .execute(SetBackgroundColor(Color::Black))? // Ensure the text is visible
            .execute(Print(burning_text))?;

        stdout.flush()?;
        sleep(Duration::from_millis(50));
    }

    // Clear the screen one last time after stopping
    stdout.execute(Clear(ClearType::All))?;
    Ok(())
}
