use std::fmt::{Display, Formatter, Pointer, Write};
use crate::logging::*;
use std::fs;
use std::ops::Deref;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;
use crate::{err_log, fatal_log, log};
use crate::archive::ArchiveEntry::{File, Folder};
use crate::core::utils::Ignore;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Regular,
    Directory,
    Symlink,
    HardLink,
    CharDevice,
    BlockDevice,
    Fifo,
}

impl Display for FileType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            FileType::Regular => "REGULAR",
            FileType::Directory => "DIRECTORY",
            FileType::Symlink => "SYMBOLIC LINK",
            FileType::HardLink => "HARD LINK",
            FileType::CharDevice => "CHAR DEVICE",
            FileType::BlockDevice => "BLOCK DEVICE",
            FileType::Fifo => "FIFO"
        };

        f.write_str(str)
    }
}

pub struct FileMetadata {
    parent_path: String, //the relative path in the archive
    name: String, //separate from path for finding purposes
    file_type: FileType,
    size: Option<u64>,
    uid: u32,
    gid: u32,
    mtime: i64,
    link_name: Option<String>,
    dev_major: Option<u32>,
    dev_minor: Option<u32>,
}


impl FileMetadata {
    pub fn retrieve(path: &PathBuf) -> Option<Self> {

        let pathstr = path.to_str().unwrap_or("{invalid path}");

        if !path.exists() {
            err_log!(false, "Failed to find file {}, skipping...", pathstr);
            return None;
        }

        // --- filesystem metadata (do NOT follow symlinks)
        let metadata = match fs::symlink_metadata(&path) {
            Ok(md) => md,
            Err(e) => {
                err_log!(false, "Failed to retrieve file metadata because of error: {} [path={}]", e.to_string(), pathstr);
                return None;
            }
        };

        log!(true, "Successfully read file metadata [path={}]", pathstr);

        let file_type = metadata.file_type();

        // --- determine logical file type
        let file_type = if file_type.is_file() {
            FileType::Regular
        } else if file_type.is_dir() {
            FileType::Directory
        } else if file_type.is_symlink() {
            FileType::Symlink
        } else {

            // unix-specific special files
            let mode = metadata.mode() & libc::S_IFMT;
            match mode {
                libc::S_IFCHR => FileType::CharDevice,
                libc::S_IFBLK => FileType::BlockDevice,
                libc::S_IFIFO => FileType::Fifo,
                _ => {
                    err_log!(false, "Unsupported file type {}, [path={}] skipping...", mode, pathstr);
                    return None;
                }
            }
        };

        log!(true, "File type: {} [path={}]", file_type, pathstr);

        // --- split path into parent + name
        let name = match path.file_name()
        {
            Some(os ) => os,
            None => {
                err_log!(false, "Failed to get filename! [path={}] skipping...", pathstr);
                return None;
            }
        }
            .to_string_lossy()
            .to_string();

        log!(true, "Filename: {} [path={}]", name, pathstr);

        let parent_path = path.parent()
            .unwrap_or(Path::new(""))
            .to_string_lossy()
            .to_string();

        log!(true, "Parent path: {} [path={}]", parent_path, pathstr);

        // --- size (only meaningful for regular files)
        let size = match file_type {
            FileType::Regular => Some(metadata.len()),
            _ => None,
        };

        log!(true, "File size: {} [path={}]",
            match size {
                Some(s) => s.to_string() + " bytes",
                None => String::from("N/A")
            }
            , pathstr);

        // --- ownership
        let uid = metadata.uid();
        let gid = metadata.gid();

        log!(true, "UID: {}, GID: {} [path={}]", uid, gid, pathstr);


        // --- modification time
        let mtime = match match metadata
            .modified() {
                Ok(o) => o,
                Err(e) => {
                    err_log!(false, "Failed to retrieve modification time [path={}] because of error: {}", pathstr, e.to_string());
                    return None;
                }
            }
            .duration_since(UNIX_EPOCH) {
                Ok(o) => o,
                Err(e) => {
                    err_log!(false, "Failed to retrieve modification time [path={}] because of error: {}", pathstr, e.to_string());
                    return None;
                }
            }
            .as_secs() as i64;

        log!(true, "Modification time stamp: {} [path={}]", mtime, pathstr);

        // --- symlink target
        let link_name = if file_type == FileType::Symlink {
            Some(
                match fs::read_link(&path)
                    {
                        Ok(o) => o,
                        Err(e) => {
                            err_log!(false, "Failed to retrieve symlink name [path={}] because of error: {}", pathstr, e.to_string());
                            return None;
                        }
                    }
                    .to_string_lossy()
                    .to_string()
            )
        } else {
            None
        };

        log!(true, "Symlink name: {} [path={}]",
            match &link_name {
                Some(s) => s.clone(),
                None => String::from("N/A")
            }
            , pathstr);

        // --- device numbers
        let (dev_major, dev_minor) = match file_type {
            FileType::CharDevice | FileType::BlockDevice => {
                let rdev = metadata.rdev();
                (
                    Some(unsafe { libc::major(rdev) }),
                    Some(unsafe { libc::minor(rdev) }),
                )
            }
            _ => (None, None),
        };

        log!(true, "Dev major: {}, Dev minor: {}, [path={}]", dev_major.unwrap_or(u32::MAX)
            , dev_minor.unwrap_or(u32::MAX)
            , pathstr);

        log!(false, "Metadata retrieved\n -parent path: \"{}\",\n -name: \"{}\",\n -type: {}", parent_path, name, file_type);

        Some( Self {
            parent_path,
            name,
            file_type,
            size,
            uid,
            gid,
            mtime,
            link_name,
            dev_major,
            dev_minor,
        } )
    }
}

pub enum ArchiveEntry {
    File(FileArchive),
    Folder(FolderArchive)
}

impl ArchiveEntry {
    pub fn add_file(&mut self, path: &PathBuf) -> Option<Arc<Mutex<ArchiveEntry>>> {
        match self {
            File(_) => None, //todo, error message?
            Folder(f) => f.add_file(path)
        }
    }

    pub fn name(&self) -> String {
        match self {
            File(f) => f.metadata.name.clone(),
            Folder(f) => f.metadata.name.clone()
        }
    }
}

impl Display for ArchiveEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            File(fl) => fl.fmt(f),
            Folder(fl) => fl.fmt(f, 0)
        }
    }
}

pub struct FileArchive {
    metadata: FileMetadata,
    bytes: Vec<u8>
}



impl Display for FileArchive {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.metadata.name.as_str())
    }
}

pub struct FolderArchive {
    metadata: FileMetadata,
    contents: Vec<Arc<Mutex<ArchiveEntry>>>,
}

impl Display for FolderArchive {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.fmt(f, 0)
    }
}

impl FolderArchive {
    pub fn add_file(&mut self, path: &PathBuf) -> Option<Arc<Mutex<ArchiveEntry>>> {
        let pathstr = path.to_str().unwrap_or("{invalid path}");

        let metadata = match FileMetadata::retrieve(&path) {
            Some(md) => md,
            None => {
                err_log!(false, "Failed to retrieve file metadata [path={}]", pathstr);
                return None;
            }
        };

        if metadata.file_type == FileType::Directory {
            self.contents.push(
                Arc::new(Mutex::new(Folder(FolderArchive{ metadata, contents: Vec::new() })))
            );
        } else {
            let byte_contents = match fs::read(path) {
                Ok(o) => o,
                Err(e) => {
                    err_log!(false, "Failed to read file because of error {} [path={}]", e, pathstr);
                    return None
                }
            };
            self.contents.push(Arc::new(Mutex::new(File(FileArchive{ metadata, bytes: byte_contents }))));
        }

        let ind = self.contents.len()-1;

        match self.contents.get_mut(ind) {
            Some(s) => Some(s.clone()),
            None => None
        }
    }

    pub fn fmt(&self, f: &mut Formatter<'_>, tabs: u32) -> std::fmt::Result {
        f.write_fmt(format_args!("{}{}/", String::from(" ").repeat(tabs as usize), self.metadata.name)).ignore();
        f.write_str("\n").ignore();
        for c in &self.contents {
            let lock = c.lock().unwrap();

            match lock.deref() {
                File(fl) => {
                    f.write_fmt(format_args!("+ {}{}",  String::from(" ").repeat((tabs+1) as usize), fl.metadata.name.clone())).ignore();
                }
                Folder(fl) => {
                    fl.fmt(f, tabs + 1).ignore();
                }
            }
            f.write_str("\n").ignore();
        }

        Ok(())
    }
}

pub struct Archive {
    entries: Vec<Arc<Mutex<ArchiveEntry>>>
}

impl Archive {
    pub fn new() -> Self {
        Self{ entries: Vec::new() }
    }

    pub fn add_file(&mut self, path: &PathBuf) -> Option<Arc<Mutex<ArchiveEntry>>> {
        let pathstr = path.to_str().unwrap_or("{invalid path}");

        let metadata = match FileMetadata::retrieve(&path) {
            Some(md) => md,
            None => {
                err_log!(false, "Failed to retrieve file metadata [path={}]", pathstr);
                return None;
            }
        };

        if metadata.file_type == FileType::Directory {
            self.entries.push(
                Arc::new(Mutex::new(Folder(FolderArchive{ metadata, contents: Vec::new() })))
            );
        } else {
            let byte_contents = match fs::read(path) {
                Ok(o) => o,
                Err(e) => {
                    err_log!(false, "Failed to read file because of error {} [path={}]", e, pathstr);
                    return None
                }
            };
            self.entries.push(Arc::new(Mutex::new(File(FileArchive{ metadata, bytes: byte_contents }))));
        }

        let ind = self.entries.len()-1;

        match self.entries.get_mut(ind) {
            Some(s) => Some(s.clone()),
            None => None
        }
    }
}

impl Display for Archive {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for entry in &self.entries {
            entry.lock().unwrap().fmt(f).ignore();
        }

        Ok(())
    }
}