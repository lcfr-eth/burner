# Burner

A rust utility for writing passes of zeros over single files, directories or disks.  

## Build
```Cargo build --release```  

## Usage
```
Usage: burner [COMMAND]

Commands:
  file  Zero a single file
  dir   Zero all files in a directory
  mbr   Overwrite the MBR of a disk with a MSG
  disk  Zero a device and optionally overwrite the MBR with a custom message
  help  Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```  

passing the ```--passes``` option to the commands specifies how many passes of zeros to write.  

## Example

Zero disk and overwrite the MBR with a message "Hai Tavis...." and 1 pass of zero writes:  

```burner disk /dev/sda --msg "Hai Tavis..:)" --passes 1```  

https://github.com/lcfr-eth/burner/assets/62533238/fbe4ba99-ff28-458b-b263-cab1e73f8daa  

Zero disk and overwrite the MBR with a message "Hai Tavis...." and 1 pass of zero writes with FIREEEEEEEEE:  

```burner disk /dev/sda --msg "Hai Tavis..:)" --passes 1 --fire```  

https://github.com/lcfr-eth/burner/assets/62533238/ae83095a-4a20-499a-a933-5a76f3351012  

## Reboot MSG

Reboot for the operator at the keyboard to be greeted with your msg and an unbootable OS :) 
