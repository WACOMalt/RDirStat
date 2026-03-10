use std::collections::HashMap;
use std::ffi::OsString;
use std::mem;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE, ERROR_HANDLE_EOF};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Ioctl::{
    FSCTL_ENUM_USN_DATA, FSCTL_QUERY_USN_JOURNAL, MFT_ENUM_DATA_V0,
    USN_JOURNAL_DATA_V0, USN_RECORD_V2, USN_RECORD_V3,
};
use windows::Win32::System::IO::DeviceIoControl;

use rdirstat_core::file_info::{FileInfo, FileKind};

use crate::options::ScanOptions;
use crate::scanner::ScanProgress;

/// Check if the current process is running with Administrator privileges.
pub fn is_admin() -> bool {
    // For now, assume true to test the MFT scan compile path
    // Properly implementing this requires a bunch of Advapi32 bindings
    true
}

pub fn collect_entries_mft(
    _options: &ScanOptions,
    progress: &Arc<ScanProgress>,
    root_path: &Path,
) -> anyhow::Result<Vec<FileInfo>> {
    // 1. Resolve drive letter from root_path
    let drive_letter = get_drive_letter(root_path)
        .ok_or_else(|| anyhow::anyhow!("Could not extract drive letter from path to scan MFT"))?;
    
    let drive_path = format!("\\\\.\\{}:", drive_letter);
    let drive_path_utf16: Vec<u16> = std::ffi::OsStr::new(&drive_path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        // 2. Open Volume Handle
        let h_vol = CreateFileW(
            PCWSTR(drive_path_utf16.as_ptr()),
            FILE_READ_ATTRIBUTES.0, // Only need attributes access for MFT
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            HANDLE::default(),
        )?;

        if h_vol == INVALID_HANDLE_VALUE {
            let err = GetLastError();
            anyhow::bail!("Failed to open volume {} for MFT scan. Error: {:?}", drive_path, err);
        }

        // 3. Query USN Journal
        let mut journal_data = USN_JOURNAL_DATA_V0::default();
        let mut bytes_returned: u32 = 0;

        let result = DeviceIoControl(
            h_vol,
            FSCTL_QUERY_USN_JOURNAL,
            None,
            0,
            Some(&mut journal_data as *mut _ as *mut std::ffi::c_void),
            mem::size_of::<USN_JOURNAL_DATA_V0>() as u32,
            Some(&mut bytes_returned),
            None,
        );

        if result.is_err() {
            let _ = CloseHandle(h_vol);
            anyhow::bail!("FSCTL_QUERY_USN_JOURNAL failed. Volume might not be NTFS.");
        }

        log::info!("USN Journal ID: 0x{:X}", journal_data.UsnJournalID);

        // 4. Enumerate MFT
        let mut mft_data = MFT_ENUM_DATA_V0 {
            StartFileReferenceNumber: 0,
            LowUsn: 0,
            HighUsn: journal_data.NextUsn,
        };

        // 64KB buffer for MFT records
        let mut buffer: Vec<u8> = vec![0; 64 * 1024];
        let mut file_infos = Vec::new();

        log::info!("Starting MFT enumeration...");

        loop {
            let result = DeviceIoControl(
                h_vol,
                FSCTL_ENUM_USN_DATA,
                Some(&mut mft_data as *mut _ as *mut std::ffi::c_void),
                mem::size_of::<MFT_ENUM_DATA_V0>() as u32,
                Some(buffer.as_mut_ptr() as *mut std::ffi::c_void),
                buffer.len() as u32,
                Some(&mut bytes_returned),
                None,
            );

            if result.is_err() {
                let err = GetLastError();
                if err == Err(windows::core::Error::from(ERROR_HANDLE_EOF)) {
                    break; // Finished
                }
                let _ = CloseHandle(h_vol);
                anyhow::bail!("FSCTL_ENUM_USN_DATA failed: {:?}", err);
            }

            // The first 8 bytes of the output buffer contain the next StartFileReferenceNumber
            let next_usn = *(buffer.as_ptr() as *const u64);
            mft_data.StartFileReferenceNumber = next_usn;

            // Iterate over records in the buffer
            let mut offset = mem::size_of::<u64>();
            while offset < bytes_returned as usize {
                let record_ptr = buffer.as_ptr().add(offset);
                
                // Read RecordLength first to know how to advance, and MajorVersion to know the struct size
                let record_length = *(record_ptr as *const u32);
                let major_version = *(record_ptr.add(4) as *const u16);

                if major_version == 2 {
                    let record = &*(record_ptr as *const USN_RECORD_V2);
                    if let Some(fi) = parse_usn_record_v2(record) {
                        file_infos.push(fi);
                        progress.files_scanned.fetch_add(1, Ordering::Relaxed);
                    }
                } else if major_version == 3 {
                    let record = &*(record_ptr as *const USN_RECORD_V3);
                    if let Some(fi) = parse_usn_record_v3(record) {
                        file_infos.push(fi);
                        progress.files_scanned.fetch_add(1, Ordering::Relaxed);
                    }
                }

                offset += record_length as usize;
            }
        }

        let _ = CloseHandle(h_vol);

        log::info!("MFT enumeration complete. building paths...");
        
        // 5. Reconstruct Tree/Paths
        // Map FRN -> &FileInfo 
        let mut frn_to_info: HashMap<u64, usize> = HashMap::with_capacity(file_infos.len());
        for (i, fi) in file_infos.iter().enumerate() {
            frn_to_info.insert(fi.inode, i); // We temporarily stored FRN in inode
        }

        // We temporarily stored ParentFileReferenceNumber in the `device` field
        for i in 0..file_infos.len() {
            let mut current_frn = file_infos[i].inode;
            let mut parent_frn = file_infos[i].device;
            let mut path_components = Vec::new();

            path_components.push(file_infos[i].name.clone());

            // Traverse up the tree until we hit the root or an unknown parent
            while current_frn != parent_frn && parent_frn != 0 {
                if let Some(&parent_idx) = frn_to_info.get(&parent_frn) {
                    path_components.push(file_infos[parent_idx].name.clone());
                    current_frn = parent_frn;
                    parent_frn = file_infos[parent_idx].device;
                } else {
                    break;
                }
            }

            path_components.push(format!("{}:\\", drive_letter));
            path_components.reverse();

            let mut out_path = PathBuf::new();
            for comp in path_components {
                out_path.push(comp);
            }

            file_infos[i].path = out_path;

            // Reset the temporary fields back to 0 just to be safe
            file_infos[i].device = 0;
            file_infos[i].depth = file_infos[i].path.components().count() as u32;
        }

        // Optimization: USN records don't give file sizes immediately (need to read MFT attributes or 
        // fallback to standard stat for files...). We will leave sizes as 0 in this MVP.

        Ok(file_infos)
    }
}

// -------------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------------

fn get_drive_letter(path: &Path) -> Option<char> {
    let path_str = path.to_string_lossy();
    if path_str.len() >= 2 && path_str.chars().nth(1) == Some(':') {
        path_str.chars().nth(0)?.to_uppercase().next()
    } else {
        None
    }
}

unsafe fn parse_usn_record_v2(record: &USN_RECORD_V2) -> Option<FileInfo> {
    // Extract filename
    let filename_ptr = (record as *const _ as *const u8).add(record.FileNameOffset as usize) as *const u16;
    let filename_len = (record.FileNameLength / 2) as usize;
    let filename_slice = std::slice::from_raw_parts(filename_ptr, filename_len);
    let name = OsString::from_wide(filename_slice).to_string_lossy().to_string();

    let kind = if record.FileAttributes & 0x00000010 != 0 { // FILE_ATTRIBUTE_DIRECTORY
        FileKind::Directory
    } else {
        FileKind::File // Basic assumption for now
    };

    let mut fi = FileInfo::new(name, PathBuf::new(), kind);
    fi.inode = record.FileReferenceNumber;
    // Store parent FRN temporarily in device field so we can reconstruct paths later
    fi.device = record.ParentFileReferenceNumber; 
    
    // USN records don't contain file size, we would need to map fragments or fallback to stat
    // For now:
    fi.size = 0; 

    Some(fi)
}

unsafe fn parse_usn_record_v3(record: &USN_RECORD_V3) -> Option<FileInfo> {
    // Extract filename
    let filename_ptr = (record as *const _ as *const u8).add(record.FileNameOffset as usize) as *const u16;
    let filename_len = (record.FileNameLength / 2) as usize;
    let filename_slice = std::slice::from_raw_parts(filename_ptr, filename_len);
    let name = OsString::from_wide(filename_slice).to_string_lossy().to_string();

    let kind = if record.FileAttributes & 0x00000010 != 0 { // FILE_ATTRIBUTE_DIRECTORY
        FileKind::Directory
    } else {
        FileKind::File
    };

    let mut fi = FileInfo::new(name, PathBuf::new(), kind);
    // V3 uses 128-bit FileReferenceNumber (a struct with 16 bytes). We'll hash it to a u64 for simplicity here.
    // In reality, we'd need a bigger inode field or robust hashing.
    fi.inode = *(record.FileReferenceNumber.Identifier.as_ptr() as *const u64);
    fi.device = *(record.ParentFileReferenceNumber.Identifier.as_ptr() as *const u64);
    fi.size = 0;

    Some(fi)
}
