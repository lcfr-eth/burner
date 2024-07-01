#![feature(io_error_more)]

mod mbr;
mod fire;

use clap::{CommandFactory, Parser, Subcommand};
use env_logger;
use log::{debug, info, warn};
use std::{
    fs::{File, OpenOptions, remove_file},
    io::{self, Write, Seek, SeekFrom},
    path::Path,
    os::fd::AsRawFd,
    thread,
    sync::{Arc, atomic::{AtomicBool, Ordering}},
};
use indicatif::{ProgressBar, ProgressStyle};
use walkdir::WalkDir;
use mbr::{MBRCODE, modify_string, write_mbr};
use fire::display_fire;
use libc::{ioctl, c_ulong};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Zero a single file
    File {
        #[arg(value_name = "FILE")]
        file: String,
        #[arg(long, value_name = "PASSES", default_value_t = 10)]
        passes: usize,
        #[arg(long)]
        rm: bool,
    },
    /// Zero all files in a directory
    Dir {
        #[arg(value_name = "DIR")]
        dir: String,
        #[arg(long, value_name = "PASSES", default_value_t = 10)]
        passes: usize,
        #[arg(long)]
        rm: bool,
    },
    /// Overwrite the MBR of a disk with a MSG
    Mbr {
        #[arg(value_name = "DISK", default_value = "/dev/sda")]
        disk: String,
        #[arg(long, value_name = "MESSAGE")]
        msg: Option<String>,
    },
    /// Zero a device and optionally overwrite the MBR with a custom message
    Disk {
        #[arg(value_name = "DEVICE")]
        device: String,
        #[arg(long, value_name = "MESSAGE")]
        msg: Option<String>,
        #[arg(long, value_name = "PASSES", default_value_t = 2)]
        passes: usize,
        #[arg(long)]
        fire: bool,
    },
}

fn get_device_size(device_path: &str) -> io::Result<u64> {
    let blkgetsize64: c_ulong = 0x80081272;
    let file = File::open(device_path)?;
    let fd = file.as_raw_fd();
    let mut size: u64 = 0;
    unsafe {
        if ioctl(fd, blkgetsize64, &mut size) != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(size)
}

fn write_zeros_to_device(device_path: &str, block_size: usize, passes: usize, fire: bool) -> io::Result<()> {
    let buffer = vec![0u8; block_size];
    let device_size = get_device_size(device_path)?;
    let mut device = OpenOptions::new().write(true).open(device_path)?;
    
    if !fire {
        let pb = ProgressBar::new(device_size);
        pb.set_style(ProgressStyle::default_bar()
            .template("{spinner:.red} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")
            .progress_chars("#>-"));
        pb.enable_steady_tick(100);

        for pass in 0..passes {
            info!("Starting pass {}/{} for device {:?}", pass + 1, passes, device_path);
            device.seek(SeekFrom::Start(0))?;  // Seek to the start of the device
            let mut written: u64 = 0;
            while written < device_size {
                let to_write = std::cmp::min(buffer.len() as u64, device_size - written) as usize;
                match device.write(&buffer[..to_write]) {
                    Ok(0) => break, // End of device
                    Ok(n) => {
                        written += n as u64;
                        pb.inc(n as u64);
                    },
                    Err(ref e) if e.kind() == io::ErrorKind::StorageFull => {
                        warn!("Device {:?} is full: {:?}", device_path, e);
                        break;
                    },
                    Err(e) => {
                        warn!("Failed to write to device {:?}: {:?}", device_path, e);
                        return Err(e);
                    }
                }
            }
            let finish_pass = format!("Pass {}/{} completed", pass + 1, passes);
            pb.finish_with_message(finish_pass);
        }
        info!("All passes completed for device {:?}", device_path);
    }

    if fire {
        for _pass in 0..passes {
            //info!("Starting pass {}/{} for device {:?}", pass + 1, passes, device_path);
            device.seek(SeekFrom::Start(0))?;  // Seek to the start of the device
            let mut written: u64 = 0;
            while written < device_size {
                let to_write = std::cmp::min(buffer.len() as u64, device_size - written) as usize;
                match device.write(&buffer[..to_write]) {
                    Ok(0) => break, // End of device
                    Ok(n) => written += n as u64,
                    Err(ref e) if e.kind() == io::ErrorKind::StorageFull => {
                        warn!("Device {:?} is full: {:?}", device_path, e);
                        break;
                    },
                    Err(e) => {
                        warn!("Failed to write to device {:?}: {:?}", device_path, e);
                        return Err(e);
                    }
                }
            }
            //info!("Pass {}/{} completed for device {:?}", pass + 1, passes, device_path);
        }
        info!("All passes completed for device {:?}", device_path);
    }
    
    Ok(())
}

fn overwrite_and_delete_file(file_path: &Path, passes: usize, delete: bool) -> io::Result<()> {
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
    if delete {
        if let Err(e) = remove_file(file_path) {
            warn!("Failed to delete file {:?}: {:?}", file_path, e);
            return Err(e);
        }
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

fn overwrite_all_files(root_path: &Path, passes: usize, delete: bool) -> io::Result<()> {
    let total_files = count_files(root_path);
    let pb = ProgressBar::new(total_files as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.red} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
        .progress_chars("#>-"));

    for entry in WalkDir::new(root_path).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            debug!("Overwriting and deleting file: {:?}", entry.path());
            if let Err(e) = overwrite_and_delete_file(entry.path(), passes, delete) {
                warn!("Failed to overwrite and delete file {:?}: {:?}", entry.path(), e);
            }
            pb.inc(1);
        }
    }
    pb.finish_with_message("All files overwritten and deleted.");
    Ok(())
}

fn main() -> io::Result<()> {
    env_logger::init();

    let args = Args::parse();

    match &args.command {
        Some(Commands::File { file, passes, rm }) => {
            let path = Path::new(file);
            info!("Starting zeroing of file {:?} with {} passes.", path, passes);
            if let Err(e) = overwrite_and_delete_file(path, *passes, *rm) {
                warn!("Failed to zero and delete file {:?}: {:?}", path, e);
            }
        },
        Some(Commands::Dir { dir, passes, rm }) => {
            let path = Path::new(dir);
            info!("Starting zero of all files in directory {:?} with {} passes.", path, passes);
            if let Err(e) = overwrite_all_files(path, *passes, *rm) {
                warn!("Failed to zero and delete all files in directory {:?}: {:?}", path, e);
            }
        },
        Some(Commands::Mbr { disk, msg }) => {
            let path = Path::new(disk);
            let mut mbr_code = MBRCODE.to_vec();

            if let Some(message) = msg.as_ref() {
                let old_msg = "Hai Tavis...";
                let new_msg = message;

                info!("Change MBR message from '{}' to '{}'", old_msg, new_msg);
                if !modify_string(&mut mbr_code, old_msg, new_msg) {
                    eprintln!("Failed to modify MBR message");
                    return Ok(());
                }
            }

            if let Err(e) = write_mbr(path, &mbr_code) {
                warn!("Failed to write MBR on {:?}: {:?}", path, e);
            }
        },
        Some(Commands::Disk { device, msg, passes, fire }) => {
            if *fire {
                let stop_flag = Arc::new(AtomicBool::new(false));
                let stop_flag_clone = Arc::clone(&stop_flag);

                let fire_thread = thread::spawn(move || {
                    if let Err(e) = display_fire(stop_flag_clone) {
                        eprintln!("Error displaying fire: {:?}", e);
                    }
                });

                let result = write_zeros_to_device(device, 1024 * 1024, *passes, *fire);

                stop_flag.store(true, Ordering::SeqCst);
                fire_thread.join().expect("Failed to join fire thread");

                if let Err(e) = result {
                    warn!("Failed to zero device {:?}: {:?}", device, e);
                }
            } else {
                info!("Starting to zero device {:?} with zeros.", device);
                if let Err(e) = write_zeros_to_device(device, 1024 * 1024, *passes, *fire) {
                    warn!("Failed to zero device {:?}: {:?}", device, e);
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
