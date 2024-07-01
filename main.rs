mod mbr;
mod fire;

use clap::{CommandFactory, Parser, Subcommand};
use env_logger;
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, info, warn};
use std::{
    fs::{OpenOptions, remove_file},
    io::{self, Write},
    path::Path,
    thread,
};
use walkdir::WalkDir;
use mbr::{MBRCODE, modify_string, write_mbr};
use fire::display_fire;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Overwrite a single file
    BurnFile {
        #[arg(value_name = "FILE")]
        file: String,
        #[arg(long, value_name = "PASSES", default_value_t = 10)]
        passes: usize,
    },
    /// Overwrite all files in a directory
    BurnDir {
        #[arg(value_name = "DIR")]
        dir: String,
        #[arg(long, value_name = "PASSES", default_value_t = 10)]
        passes: usize,
    },
    /// Overwrite the MBR of a disk
    WriteMbr {
        #[arg(value_name = "DISK", default_value = "/dev/sda")]
        disk: String,
        #[arg(long, value_name = "MESSAGE")]
        msg: Option<String>,
    },
    /// Wipe all files and overwrite the MBR with a custom message
    BurnDisk {
        #[arg(value_name = "DEVICE")]
        device: String,
        #[arg(long, value_name = "MESSAGE")]
        msg: Option<String>,
        #[arg(long, value_name = "PASSES", default_value_t = 10)]
        passes: usize,
        #[arg(long)]
        fire: bool,
    },
}

fn write_zeros_to_device(device_path: &str, block_size: usize, passes: usize) -> io::Result<()> {
    let mut device = OpenOptions::new().write(true).open(device_path)?;
    let buffer = vec![0u8; block_size];
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::default_spinner().template("{spinner:.red} [{elapsed_precise}] {msg}"));
    pb.enable_steady_tick(100);
    for pass in 0..passes {
        info!("Starting pass {}/{} for device {:?}", pass + 1, passes, device_path);
        loop {
            match device.write(&buffer) {
                Ok(0) => break, // End of device
                Ok(_) => pb.inc(1),
                Err(e) => {
                    warn!("Failed to write to device {:?}: {:?}", device_path, e);
                    return Err(e);
                }
            }
        }
        //pb.finish_with_message(&format!("Pass {}/{} completed.", pass + 1, passes));
    }
    pb.finish_with_message("Device formatted.");
    //pb.finish_with_message(&format!("Pass {}/{} completed.", pass + 1, passes));
    info!("All passes completed for device {:?}", device_path);
    Ok(())
}
/*
fn write_zeros_to_device(device_path: &str, block_size: usize, passes: usize) -> io::Result<()> {
    let mut device = OpenOptions::new().write(true).open(device_path)?;
    let buffer = vec![0u8; block_size];

    for pass in 0..passes {
        info!("Starting pass {}/{} for device {:?}", pass + 1, passes, device_path);
        loop {
            match device.write(&buffer) {
                Ok(0) => break, // End of device
                Ok(_) => (),
                Err(e) => {
                    warn!("Failed to write to device {:?}: {:?}", device_path, e);
                    return Err(e);
                }
            }
        }
    }

    info!("All passes completed for device {:?}", device_path);
    Ok(())
}
*/

fn overwrite_and_delete_file(file_path: &Path, passes: usize) -> io::Result<()> {
    let metadata = match file_path.metadata() {
        Ok(metadata) => metadata,
        Err(e) => {
            warn!("Failed to get metadata for file {:?}: {:?}", file_path, e);
            return Err(e);
        }
    };
    let file_size = metadata.len() as usize;

    for _ in 0..passes {
        let mut file = match OpenOptions::new().write(true).open(file_path) {
            Ok(file) => file,
            Err(e) => {
                debug!("Failed to open file {:?}: {:?}", file_path, e);
                return Err(e);
            }
        };

        let zero_bytes = vec![0u8; file_size];
        if let Err(e) = file.write_all(&zero_bytes) {
            warn!("Failed to write to file {:?}: {:?}", file_path, e);
            return Err(e);
        }
        if let Err(e) = file.sync_all() {
            warn!("Failed to sync file {:?}: {:?}", file_path, e);
            return Err(e);
        }
    }

    if let Err(e) = remove_file(file_path) {
        warn!("Failed to delete file {:?}: {:?}", file_path, e);
        return Err(e);
    }

    Ok(())
}

fn count_files(root_path: &Path) -> usize {
    WalkDir::new(root_path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .count()
}

fn overwrite_all_files(root_path: &Path, passes: usize) -> io::Result<()> {
    let total_files = count_files(root_path);
    let pb = ProgressBar::new(total_files as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.red} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
        .progress_chars("#>-"));

    for entry in WalkDir::new(root_path).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            debug!("Overwriting and deleting file: {:?}", entry.path());
            if let Err(e) = overwrite_and_delete_file(entry.path(), passes) {
                warn!("Failed to overwrite and delete file {:?}: {:?}", entry.path(), e);
            }
            pb.inc(1);
        }
    }

    pb.finish_with_message("All files overwritten and deleted.");
    Ok(())
}

fn main() -> io::Result<()> {
    // Initialize the logger
    env_logger::init();

    let args = Args::parse();

    match &args.command {
        Some(Commands::BurnFile { file, passes }) => {
            let path = Path::new(file);
            info!("Starting overwrite of file {:?} with {} passes.", path, passes);
            if let Err(e) = overwrite_and_delete_file(path, *passes) {
                warn!("Failed to overwrite and delete file {:?}: {:?}", path, e);
            }
        },
        Some(Commands::BurnDir { dir, passes }) => {
            let path = Path::new(dir);
            info!("Starting overwrite of all files in directory {:?} with {} passes.", path, passes);
            if let Err(e) = overwrite_all_files(path, *passes) {
                warn!("Failed to overwrite and delete all files in directory {:?}: {:?}", path, e);
            }
        },
        Some(Commands::WriteMbr { disk, msg }) => {
            let path = Path::new(disk);
            let mut mbr_code = MBRCODE.to_vec();

            if let Some(message) = msg.as_ref() {
                let old_msg = "Hai Tavis...";
                let new_msg = message;

                info!("Modifying MBR message from '{}' to '{}'", old_msg, new_msg);
                if !modify_string(&mut mbr_code, old_msg, new_msg) {
                    eprintln!("Failed to modify MBR message");
                    return Ok(());
                }
            }

            if let Err(e) = write_mbr(path, &mbr_code) {
                warn!("Failed to write MBR on {:?}: {:?}", path, e);
            }
        },
        Some(Commands::BurnDisk { device, msg, passes, fire }) => {
            if *fire {
                // Run fire animation in a separate thread
                let fire_thread = thread::spawn(move || {
                    if let Err(e) = display_fire() {
                        eprintln!("Error displaying fire: {:?}", e);
                    }
                });

                // Format device and wait for fire animation to finish
                let result = write_zeros_to_device(device, 1024 * 1024, *passes);
                fire_thread.join().expect("Failed to join fire thread");
                if let Err(e) = result {
                    warn!("Failed to format device {:?}: {:?}", device, e);
                }
            } else {
                // Just format device
                info!("Starting to format device {:?} with zeros.", device);
                if let Err(e) = write_zeros_to_device(device, 1024 * 1024, *passes) {
                    warn!("Failed to format device {:?}: {:?}", device, e);
                }
            }

            if let Some(message) = msg.as_ref() {
                let path = Path::new(device);
                let mut mbr_code = MBRCODE.to_vec();

                let old_msg = "Hai Tavis...";
                let new_msg = message;

                info!("Modifying MBR message from '{}' to '{}'", old_msg, new_msg);
                if !modify_string(&mut mbr_code, old_msg, new_msg) {
                    eprintln!("Failed to modify MBR message");
                    return Ok(());
                }

                if let Err(e) = write_mbr(path, &mbr_code) {
                    warn!("Failed to write MBR on {:?}: {:?}", path, e);
                }
            }
        },
        None => {
            Args::command().print_help().unwrap();
            std::process::exit(0);
        }
    }

    println!("Finished.");
    Ok(())
}
