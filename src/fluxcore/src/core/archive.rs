use std::io::Write as write_trait;
use std::collections::VecDeque;
use std::fmt::{Display, Formatter};
use crate::logging::*;
use std::{env, fs, io};
use std::ops::Deref;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;
use crate::{err_log, log};
use crate::archive::ArchiveEntry::{File, Folder};
use crate::archive::FileType::{BlockDevice, CharDevice, Directory, Fifo, HardLink, Regular, Symlink, Unknown};
use crate::core::utils::Ignore;

pub struct RecursiveFolderIterator {
    to_iterate: VecDeque<PathBuf>
}

impl RecursiveFolderIterator {
    pub fn new(path: PathBuf) -> Option<Self> {
        if path.is_dir() {
            let mut res = Self{ to_iterate: VecDeque::new() };

            for entry in match fs::read_dir(path) {
                Ok(d) => d,
                Err(_) => {
                    return None;
                }
            } {
                let entry = match entry {
                    Ok(e) => e,
                    Err(_) => {
                        return None;
                    }
                };

                res.to_iterate.push_back(entry.path())
            }

            Some( res )
        } else {
            None
        }
    }
}

impl Iterator for RecursiveFolderIterator {
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.to_iterate.pop_front()?;



        if item.is_dir() {
            for entry in match fs::read_dir(item.clone()) {
                Ok(d) => d,
                Err(_) => { return Some(item) }
            } {
                let entry = match entry {
                    Ok(e) => e,
                    Err(_) => {
                        continue;
                    }
                };

                self.to_iterate.push_back(entry.path())
            }
        }

        Some(item)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FileType {
    Regular = 0,
    Directory = 1,
    Symlink = 2,
    HardLink = 3,
#[cfg(unix)] CharDevice = 4,
#[cfg(unix)] BlockDevice = 5,
#[cfg(unix)] Fifo = 6,
    Unknown = 255
}

impl From<u8> for FileType {
    fn from(value: u8) -> Self {
        match value {
            0 => Regular,
            1 => Directory,
            2 => Symlink,
            3 => HardLink,
#[cfg(unix)] 4 => CharDevice,
#[cfg(unix)] 5 => BlockDevice,
#[cfg(unix)] 6 => Fifo,
            _ => Unknown
        }
    }
}

impl Default for FileType {
    fn default() -> Self {
        Self::Unknown
    }
}

impl Display for FileType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            FileType::Regular => "REGULAR",
            FileType::Directory => "DIRECTORY",
            FileType::Symlink => "SYMLINK",
            FileType::HardLink => "HARDLINK",
#[cfg(unix)] FileType::CharDevice => "CHARDEVICE",
#[cfg(unix)] FileType::BlockDevice => "BLOCKDEVICE",
#[cfg(unix)] FileType::Fifo => "FIFO",
            FileType::Unknown => "UNKNOWN",
        };

        f.write_str(str)
    }
}

pub struct FileMetadata {
    pub parent_path: String, //the relative path in the archive
    pub name: String, //separate from path for finding purposes
    pub file_type: FileType,
    pub size: Option<u64>, //subject to change when we actually read the file
    pub uid: u32,
    pub gid: u32,
    pub mtime: i64,
    pub mode: Option<u32>,
    pub link_name: Option<String>,
    pub dev_major: Option<u32>,
    pub dev_minor: Option<u32>,
}



impl Default for FileMetadata {
    fn default() -> Self {
        Self{
            parent_path: String::new(),
            name: String::new(),
            file_type: FileType::Unknown,
            size: None,
            uid: 0,
            gid: 0,
            mtime: 0,
            link_name: None,
            mode: Some(0o777),
            dev_major: None,
            dev_minor: None
        }
    }
}

impl FileMetadata {

    pub fn full_path(&self) -> String {
        let mut path = self.parent_path.clone();
        path = path + "/" + self.name.as_str();

        path
    }

    pub fn simple(parent_path: String, name: String, file_type: FileType) -> Self {
        Self{
            parent_path,
            name,
            file_type,
            size: None,
            uid: 0,
            gid: 0,
            mtime: 0,
            mode: None,
            link_name: None,
            dev_major: None,
            dev_minor: None
        }
    }

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


        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;
        
        #[cfg(unix)]
        let mode = Some(metadata.permissions().mode());
        
        #[cfg(not(unix))]
        let mode = None;
        

        let file_type = metadata.file_type();

        // --- determine logical file type
        let file_type = if file_type.is_file() {
            FileType::Regular
        } else if file_type.is_dir() {
            FileType::Directory
        } else if file_type.is_symlink() {
            FileType::Symlink
        } else {
            #[cfg(unix)] {

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
        #[cfg(unix)] let (dev_major, dev_minor) = match file_type {
            FileType::CharDevice | FileType::BlockDevice => {
                let rdev = metadata.rdev();
                (
                    Some( libc::major(rdev) ),
                    Some( libc::minor(rdev) ),
                )
            }
            _ => (None, None),
        };

        #[cfg(unix)] log!(true, "Dev major: {}, Dev minor: {}, [path={}]", dev_major.unwrap_or(u32::MAX)
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
            mode,
#[cfg(unix)] dev_major, #[cfg(not(unix))] None,
#[cfg(unix)] dev_minor, #[cfg(not(unix))] None,
        } )
    }
}

pub enum ArchiveEntry {
    File(FileArchive),
    Folder(FolderArchive)
}

impl ArchiveEntry {
    
    pub fn fullpath(&self) -> String {
        match self { 
            File(fa) => fa.full_path(),
            Folder(fa) => fa.full_path()
        }
    }
    
   pub fn name(&self) -> String {
       match self {
           File(fa) => fa.name(),
           Folder(fa) => fa.name(),
       }
   }

    pub fn is_file(&self) -> bool {
        match self {
            File(_) => true,
            Folder(_) => false
        }
    }

    pub fn is_folder(&self) -> bool {
        match self {
            File(_) => false,
            Folder(_) => true
        }
    }

    pub fn as_file(&self) -> Option<&FileArchive> {
        match self {
            Folder(_) => None,
            File(fa) => Some(fa)
        }
    }

    pub fn as_file_mut(&mut self) -> Option<&mut FileArchive> {
        match self {
            Folder(_) => None,
            File(fa) => Some(fa)
        }
    }

    pub fn as_folder(&self) -> Option<&FolderArchive> {
        match self {
            File(_) => None,
            Folder(fa) => Some(fa)
        }
    }

    pub fn as_folder_mut(&mut self) -> Option<&mut FolderArchive> {
        match self {
            File(_) => None,
            Folder(fa) => Some(fa)
        }
    }
}



pub struct FileArchive {
    pub metadata: FileMetadata,
    pub bytes: Vec<u8>
}

impl FileArchive {
    pub fn new(metadata: FileMetadata) -> Self {
        Self{ metadata, bytes: Vec::new() }
    }

    pub fn empty() -> Self {
        Self{ metadata: FileMetadata::default(), bytes: Vec::new() }
    }

    pub fn name(&self) -> String {
        self.metadata.name.clone()
    }

    pub fn full_path(&self) -> String {
        self.metadata.parent_path.clone() + "/" + &*self.metadata.name
    }
}




pub fn canonicalize_logical(pathref: impl AsRef<Path>) -> PathBuf {
    let path = pathref.as_ref();
    let mut out = PathBuf::new();

    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => { out.pop(); },
            Component::Normal(name) => { out.push(name); }
            Component::RootDir | Component::Prefix(_) => {}
        }
    }

    out
}

pub fn comp_to_str(component: Component) -> Option<String> {
    match component {
        Component::Normal(n) => { match n.to_str() {
            Some(s) => Some(String::from(s)),
            None => {
                None
            }
        } }
        Component::Prefix(_) | Component::RootDir | Component::CurDir | Component::ParentDir =>
            { /* shouldnt happen bcus of canonicalize_logical */ Some(String::from("it still happened stupid")) }
    }
}

pub struct FolderArchive {
    pub metadata: FileMetadata, //so we can also use FolderArchive as a regular container
    pub contents: Vec<Arc<Mutex<ArchiveEntry>>>,
    pub is_container: bool,
}

impl FolderArchive {

    pub fn len(&self) -> usize {
        self.contents.len()
    }

    pub fn name(&self) -> String {
        self.metadata.name.clone()
    }
    pub fn container() -> Self {
        Self{ metadata: FileMetadata{
            name: String::from("archive_root"),
            file_type: FileType::Directory,
            ..Default::default()
        }, contents: Vec::new(), is_container: true }
    }

    pub fn with_metadata(metadata: FileMetadata) -> Self {
        Self { metadata, contents: Vec::new(), is_container: false }
    }

    pub fn full_path(&self) -> String {
        self.metadata.parent_path.clone() + "/" + &*self.metadata.name
    }

    pub fn using_path(mut metadata: FileMetadata, parent_path: String, name: String) -> Self {
        metadata.parent_path = parent_path;
        metadata.name = name;

        Self::with_metadata(metadata)
    }

    pub fn find_top_level(&self, name: &String) -> Option<Arc<Mutex<ArchiveEntry>>> {

        for c in &self.contents {
            let mut found = false;
            {
                let lock = c.lock().unwrap();
                if lock.name() == *name {
                    found = true;
                }
            }

            if found {
                return Some(c.clone())
            }
        }

        None
    }

    pub fn make_metadata(&self, path: &PathBuf) -> Option<FileMetadata> {
        let pathstr = path.to_str().unwrap_or("{invalid path}");
        let mut metadata = match FileMetadata::retrieve(&PathBuf::from(path.to_str().unwrap_or("{invalid path}"))) {
            Some(md) => md,
            None => {
                err_log!(false, "Failed to find file {} relative to the CWD", pathstr);
                return None;
            }
        };



        //make the path relative to this folder
        metadata.parent_path = self.full_path();

        Some(metadata)
    }

    pub fn find_top_level_or_add_directly(&mut self, path: &PathBuf) -> Option<Arc<Mutex<ArchiveEntry>>>  {

        let metadata = self.make_metadata(path)?;

        match self.find_top_level(&metadata.name) {
            Some(a) => Some(a),
            None => Some(self.add_directly(path)?)
        }
    }



    fn __add_directly(&mut self, filename: &PathBuf) -> Option<Arc<Mutex<ArchiveEntry>>> {
        let pathstr = filename.to_str().unwrap_or("{invalid path}");
        //looks for the file in the CWD
        let metadata = self.make_metadata(filename)?;

        //make sure we dont already have the file
        if let Some(_) = self.find_top_level(&metadata.name) {
            err_log!(false, "Cannot add file {} directly because it already exists in directory [path={}]!", pathstr, self.full_path());
            return None;
        }

        if metadata.file_type == Directory {
            //we don't add folders recursively in add_directly
            let res = Arc::new(Mutex::new(Folder( FolderArchive::with_metadata(metadata) )));

            self.contents.push(res.clone());

            Some(res)
        } else {
            let mut file = FileArchive::new(metadata);

            let bytes = fs::read(filename).unwrap_or(Vec::new());

            file.metadata.size = Some(bytes.len() as u64);
            file.bytes = bytes;

            let res = Arc::new(Mutex::new(File( file )));

            self.contents.push(res.clone());

            Some(res)
        }
    }

    pub fn add_directly(&mut self, filename: &PathBuf) -> Option<Arc<Mutex<ArchiveEntry>>> {
        let filename = canonicalize_logical(filename);
        self.__add_directly(&filename)
    }

    fn __add_directly_recursively(&mut self, filename: &PathBuf) -> Option<Arc<Mutex<ArchiveEntry>>> {
        let cur = self.__add_directly(&filename)?;


        let is_folder = cur.lock().unwrap().is_folder();

        if is_folder {

            if let Ok(files) = fs::read_dir(filename) {
                for file in files {
                    let Ok(file) = file else {
                        continue;
                    };

                    let mut lock = cur.lock().unwrap();

                    lock.as_folder_mut().unwrap().add_directly_recursively(&file.path());
                }
            }
        }

        Some(cur.clone())
    }

    pub fn add_directly_recursively(&mut self, filename: &PathBuf) -> Option<Arc<Mutex<ArchiveEntry>>> {
        let filename = canonicalize_logical(filename);

        self.__add_directly_recursively(&filename)
    }


    pub fn add(&mut self, filename: &PathBuf) -> Option<Arc<Mutex<ArchiveEntry>>>  {
        let filename = canonicalize_logical(filename);
        let pathstr = filename.to_str().unwrap_or("{invalid path}");
        //get a relative path
        let rel = if !self.is_container {
            match filename.strip_prefix(self.full_path()) {
                Ok(a) => a,
                Err(e) => {
                    err_log!(false, "Failed to add file {} because it is not relative to this folder [{}]\nnote: {}", pathstr, self.full_path()
                        , e.to_string());
                    return None;
                }
            }
        } else {
            filename.as_path()
        };


        let mut components = rel.components().peekable();

        let mut cur_path: PathBuf = if !self.is_container {
            PathBuf::from(self.full_path())
        } else {
            PathBuf::new()
        };

        let mut cur = {
            let name = comp_to_str(components.next()?)?;
            cur_path.push(name.clone());

            self.find_top_level_or_add_directly(&cur_path)?
        };

        while let Some(c) = components.next() {
            let name = comp_to_str(c)?;

            {
                let lock = cur.lock().unwrap();

                let is_dir = lock.is_folder();
                if !is_dir {
                    err_log!(false, "Failed to add path {} because one component referenced a directory when it was actually a file!", pathstr);
                }


                cur_path.push(name.clone());
            }

            cur = {
                let res = cur.lock().unwrap().as_folder_mut()?.find_top_level_or_add_directly(&cur_path);
                res?
            }
        }

        //now if cur is a folder, we add all its contents recursively
        let is_dir = cur.lock().unwrap().is_folder();

        if is_dir {
            if let Ok(files) = fs::read_dir(filename) {
                for file in files {
                    let Ok(file) = file else {
                        continue;
                    };

                    cur.lock().unwrap().as_folder_mut()?.__add_directly_recursively(&file.path());
                }
            }
        }

        Some(cur)
    }

    fn format(&self, f: &mut Formatter<'_>, depth: usize) -> std::fmt::Result {
        const TAB: &str = "••";
        f.write_str(self.name().as_str()).ignore();
        f.write_str("/\n").ignore();

        for c in &self.contents {
            let lock = c.lock().unwrap();

            f.write_str(TAB.repeat(depth+1).as_str()).ignore();
            if lock.is_file() {
                f.write_str(
                    lock.as_file().unwrap().name().as_str()
                ).ignore();
            } else {
                lock.as_folder().unwrap().format(f, depth+1).ignore();
            }
            f.write_str("\n").ignore();
        }

        Ok(())
    }

    pub fn flatten(&self) -> Vec<Arc<Mutex<ArchiveEntry>>> {
        let mut res = Vec::new();

        let mut stack: Vec<_> = self.contents
            .iter()
            .map(
                |c| c.clone()
            )
            .collect();

        while let Some(item) = stack.pop() {

            res.push(item.clone());

            let is_dir = item.lock().unwrap().is_folder();

            if is_dir {
                stack.extend(
                    item.lock().unwrap().as_folder().unwrap().contents.clone()
                );
            }
        }

        res
    }



    pub fn output_directory(&self) -> io::Result<()> {
        use filetime::*;
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;
        #[cfg(unix)]
        use nix::unistd::{chown, Uid, Gid};

        let cwd = env::current_dir()?.to_string_lossy().to_string();

        let dir_path = PathBuf::from(cwd.clone() + self.full_path().as_str());
        fs::create_dir_all(dir_path.clone())?;


        for file in &self.contents {
            let lock = file.lock().unwrap();

            match lock.deref() {
                File(filearc) => {
                    let path = PathBuf::from(cwd.clone() + filearc.full_path().as_str());
                    let mut output_file = fs::File::create(&path)?;
                    output_file.write(filearc.bytes.as_slice())?;

                    let mtime = FileTime::from_unix_time(self.metadata.mtime, 0);
                    set_file_times(&path, mtime, mtime)?;

                    #[cfg(unix)]
                    {
                        if let Some(mode) = self.metadata.mode {
                            let mut perms = fs::metadata(&path)?.permissions();
                            perms.set_mode(mode);
                            fs::set_permissions(&path, perms)?;
                        }

                        let _ = chown(
                            &path,
                            Some(Uid::from_raw(self.metadata.uid)),
                            Some(Gid::from_raw(self.metadata.gid))
                        );
                    }

                }
                Folder(folderarc) => {
                    folderarc.output_directory()?;


                }
            }



        }

        let mtime = FileTime::from_unix_time(self.metadata.mtime, 0);
        set_file_times(&dir_path, mtime, mtime)?;

        #[cfg(unix)]
        {
            if let Some(mode) = self.metadata.mode {
                let mut perms = fs::metadata(&dir_path)?.permissions();
                perms.set_mode(mode);
                fs::set_permissions(&dir_path, perms)?;
            }

            let _ = chown(
                &dir_path,
                Some(Uid::from_raw(self.metadata.uid)),
                Some(Gid::from_raw(self.metadata.gid))
            );
        }

        Ok(())
    }
}

impl Display for FolderArchive {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.format(f, 0)
    }
}
