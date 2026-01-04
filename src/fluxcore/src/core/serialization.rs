use crate::logging::{log, ProgressLogger};
use std::fs::File;
use crate::logging::err_log;
use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::{fs, io, ptr};
use std::ffi::OsString;
use std::io::{Error, Write};
use std::sync::{Arc, Mutex};
use crc64::crc64;
use crate::archive::{ArchiveEntry, FileArchive, FileMetadata, FileType, FolderArchive};
use crate::{err_log, log};
use crate::archive::ArchiveEntry::{Folder};
use crate::utils::{Ignore, ProgressTracker};

pub struct Bytes {
    buf: Vec<u8>,
    read_pos: usize,
}

impl Bytes {
    pub fn new() -> Self {
        Self{ buf: Vec::new(), read_pos: 0 }
    }

    pub fn from_iter(x: impl IntoIterator<Item=u8>) -> Self {
        let mut res = Self::new();
        for i in x.into_iter() {
            res.push(i);
        }

        res
    }

    pub fn from_raw_bytes<T: Copy>(val: &T) -> Self {
        let mut res = Self::new();
        res.raw_bytes(val);

        res
    }

    pub fn set_read_position(&mut self, pos: usize) {
        self.read_pos = pos;
    }

    pub fn raw_bytes<T: Copy>(&mut self, val: &T) {
        let bytes = unsafe {
            std::slice::from_raw_parts(
                val as *const T as *const u8,
                size_of::<T>()
            )
        };
        self.buf.extend_from_slice(bytes);
    }


    //note: this is only well-defined for types that can be represented by pure bytes
    //if a type contains a reference, pointer, or anything that isnt pure data
    //this is undefined behavior, even so the byte layout may not match what
    //you are expecting, so accessing T after calling this function is still undefined
    //and not recommended for anything besides numerical types
    pub unsafe fn bytes_into<T: Copy>(&mut self) -> Option<T> {
        let size = size_of::<T>();
        let bytes = self.next_n(size);

        if bytes.len() != size {
            return None;
        }

        let mut out = MaybeUninit::<T>::uninit();

        unsafe {
            ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                out.as_mut_ptr() as *mut u8,
                size,
            );
        }

        Some(unsafe{ out.assume_init() })
    }

    pub fn get_read_position(&self) -> usize {
        self.read_pos
    }

    pub fn next_n(&mut self, n: usize) -> Vec<u8> {
        let mut res = Vec::new();
        let mut i = 0;
        while i < n && let Some(b) = self.next() {
            i += 1;
            res.push(b);
        }

        res
    }
}

impl Iterator for Bytes {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        if self.read_pos >= self.buf.len() {
            None
        } else {
            let res = self.buf[self.read_pos];
            self.read_pos += 1;
            Some(res)
        }
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.read_pos += n;

        self.next()
    }
}

impl Deref for Bytes {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.buf
    }
}

impl DerefMut for Bytes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buf
    }
}

pub struct FluxFile {
    bytes: Bytes
}

//todo: implement compression
#[repr(u8)]
pub enum CompressionMode {
    Error = 255,
    None = 0,
}

impl From<u8> for CompressionMode {
    fn from(value: u8) -> Self {
        match value {
            0 => CompressionMode::None,
            _ => CompressionMode::Error,
        }
    }
}

const HEADER_SIZE: usize = 13;

impl FluxFile {

    pub fn save(&self, filename: impl AsRef<Path>) -> io::Result<()> {
        let mut file = File::create(filename)?;

        file.write_all(self.bytes.as_slice())
    }

    pub fn load(filename: impl AsRef<Path>) -> Result<Self, String> {
        let pthstr = filename.as_ref().to_str().unwrap_or("{invalid path}");

        //loads the file, checks that the file is well-formed and decompresses if needed
        let bytes = match fs::read(filename) {
            Ok(v) => v,
            Err(e) => {
                return Err(e.to_string());
            }
        };

        let mut bytes = Bytes::from_iter(bytes);

        if bytes.len() < HEADER_SIZE {
            return Err("File is too small to be an archive".to_string());
        }

        //check that the file is well formed by parsing through the first
        if str::from_utf8(bytes.next_n(4).as_slice()).unwrap_or("") != "FLUX" {
            return Err(String::from("File is missing proper header"));
        }

        let compression_mode = CompressionMode::from(bytes.next().unwrap_or(255));




        let checksum = u64::from_le_bytes(match bytes.next_n(8).as_slice().try_into() {
            Ok(u) => u,
            Err(e) => {
                return Err("Could not get file checksum!".to_string())
            }
        });

        if bytes.len() == HEADER_SIZE {
            return Ok(Self{ bytes })
        }

        let remaining_data = &(bytes.as_slice()[HEADER_SIZE..]);

        //todo: decompression
        match compression_mode {
            CompressionMode::Error => {
                return Err("Bad compression byte".to_string());
            },
            CompressionMode::None => {}
        }

        let crc = crc64(0, remaining_data);

        if crc != checksum {
            return Err(format!("Checksums do not match! ({} != {})", checksum, crc));
        }

        //at this point, the data is perfectly valid and we can go ahead and return Ok
        Ok(Self{ bytes })
    }

    pub fn serialize(mut archive: FolderArchive, compression_mode: CompressionMode) -> Option<FluxFile> {
        
        //top-level folder should have a parent path
        archive.metadata.parent_path = String::new();
        
        let mut bytes = Bytes::new();

        bytes.extend_from_slice("FLUX".as_bytes());
        bytes.push(compression_mode as u8);


        //folder archives are serialized as data sections, first we need to make that data
        let data = match Self::serialize_folder(&archive, 0) {
            Ok(d) => d,
            Err(s) => {
                err_log!(false, "Failed to serialize archive due to error: {}", s);
                return None;
            }
        };

        //combine the data
        let mut combined = Bytes::new();
        combined.extend_from_slice("DATA".as_bytes());
        combined.raw_bytes(&(data.len() as u64));
        combined.extend(data);

        //compute the CRC64 on the data
        let checksum = crc64(0, combined.as_slice());

        bytes.extend_from_slice(&checksum.to_le_bytes());

        log!(true, "data CRC64/ISO checksum: {}", checksum);

        //todo: compression

        //add the combined data
        bytes.extend(combined);


        Some( Self{ bytes } )
    }

    pub fn set_serialized_metadata_filesize(mut metadata: Bytes, size: u64) -> Option<Bytes> {
        let bytes = Bytes::from_iter(size.to_le_bytes());

        if metadata.len() < 8 {
            err_log!(true, "Failed to change serialized metadata size!");
            return None;
        }

        for i in 0..8 {
            metadata[i] = bytes[i];
        }

        Some(metadata)
    }

    pub fn serialize_metadata(metadata: &FileMetadata) -> Bytes {
        let mut res = Bytes::new();

        //file size
        res.extend_from_slice(&metadata.size.unwrap_or(0).to_le_bytes());

        //full path
        let full_path = metadata.full_path();
        res.extend_from_slice(&(full_path.len() as u32).to_le_bytes());
        res.extend_from_slice(full_path.as_bytes());

        //file type
        res.push(metadata.file_type as u8);

        //uid
        res.extend_from_slice(&metadata.uid.to_le_bytes());

        //gid
        res.extend_from_slice(&metadata.gid.to_le_bytes());

        //mtime
        res.extend_from_slice(&metadata.mtime.to_le_bytes());

        //mode
        res.extend_from_slice(&metadata.mode.unwrap_or(0).to_le_bytes());

        //link_name_len
        let link_name = metadata.link_name.clone().unwrap_or(String::new());
        res.extend_from_slice(&(link_name.len() as u32).to_le_bytes());
        res.extend_from_slice(link_name.as_bytes());

        //dev major
        res.extend_from_slice(&(metadata.dev_major.unwrap_or(0)).to_le_bytes());
        res.extend_from_slice(&(metadata.dev_minor.unwrap_or(0)).to_le_bytes());

        res
    }


    //depth is for logging purposes
    pub fn serialize_folder(archive: &FolderArchive, depth: u32) -> Result<Bytes, String> {

        

        let fullpath = archive.full_path();

        log!(true, "Serializing {}{}-order folder [content path = {}]", depth,
            match depth % 10 {
                1 => "st",
                2 => "nd",
                3 => "rd",
                _ => "th"
            }, fullpath
        );
        let mut pt = ProgressTracker::new(archive.contents.len() as u64);

        log!(true, "Serializing metadata for folder [content path = {}]", fullpath);
        //once we parse we have to change the size stored here
        let mut smetadata = Self::serialize_metadata(&archive.metadata);



        let mut data = Bytes::new();

        //collect data
        for c in &archive.contents {
            let pth = c.lock().unwrap().fullpath();
            log!(true, "Serializing entry for folder [{}]", pth);
            let lock = c.lock().unwrap();
            let bytes = match lock.deref() {
                ArchiveEntry::File(fa) => Self::serialize_file(fa),
                ArchiveEntry::Folder(fa) => Self::serialize_folder(fa, depth+1)?
            };

            data.extend(bytes);
            pt.inc();
            println!("{}/{} ({:.1}%) sub-contents serialized [parent = {}]", pt.cur(), pt.max(), pt.percent()*100.0, pth)
        }

        log!(true, "Folder size: {} [content path = {}]", data.len(), fullpath);
        smetadata = match Self::set_serialized_metadata_filesize(smetadata, data.len() as u64) {
            Some(sm) => sm,
            None => {
                return Err(String::from("Failed to serialize archive due to bad metadata serialization!"));
            }
        };

        smetadata.extend(data);

        log!(false, "Successfully serialized folder [content path = {}]", fullpath);
        Ok(smetadata)
    }

    pub fn serialize_file(file: &FileArchive) -> Bytes {
        let fullpath = file.full_path();
        log!(true, "Serializing file [content path = {}]", fullpath);
        let mut smetadata = Self::serialize_metadata(&file.metadata);

        //add the file data
        smetadata.extend(&file.bytes);

        log!(false, "Successfully serialized file [content path = {}, size = {}]", fullpath, smetadata.len());
        smetadata
    }

    fn deserialize_metadata(bytes: &mut Bytes) -> Result<FileMetadata, String> {
        let filesize = u64::from_le_bytes(
            match bytes.next_n(8).as_slice().try_into() {
                Ok(b) => b,
                Err(e) => {
                    return Err(e.to_string());
                }
            }
        );

        let full_path_len = u32::from_le_bytes(
            match bytes.next_n(4).as_slice().try_into() {
                Ok(b) => b,
                Err(e) => {
                    return Err(e.to_string());
                }
            }
        );

        let full_path = PathBuf::from(String::from_utf8_lossy(
            bytes.next_n(full_path_len as usize).as_slice()
        ).to_string());

        let file_type = FileType::from(bytes.next().unwrap_or(255));

        let uid = u32::from_le_bytes(
            match bytes.next_n(4).as_slice().try_into() {
                Ok(b) => b,
                Err(e) => {
                    return Err(e.to_string());
                }
            }
        );

        let gid = u32::from_le_bytes(
            match bytes.next_n(4).as_slice().try_into() {
                Ok(b) => b,
                Err(e) => {
                    return Err(e.to_string());
                }
            }
        );

        let mtime = i64::from_le_bytes(
            match bytes.next_n(8).as_slice().try_into() {
                Ok(b) => b,
                Err(e) => {
                    return Err(e.to_string());
                }
            }
        );

        let mode = u32::from_le_bytes(
            match bytes.next_n(4).as_slice().try_into() {
                Ok(b) => b,
                Err(e) => {
                    return Err(e.to_string());
                }
            }
        );

        let link_name_len = u32::from_le_bytes(
            match bytes.next_n(4).as_slice().try_into() {
                Ok(b) => b,
                Err(e) => {
                    return Err(e.to_string());
                }
            }
        );

        let link_name = String::from_utf8_lossy(
            bytes.next_n(link_name_len as usize).as_slice()
        ).to_string();

        let dev_major = u32::from_le_bytes(
            match bytes.next_n(4).as_slice().try_into() {
                Ok(b) => b,
                Err(e) => {
                    return Err(e.to_string());
                }
            }
        );

        let dev_minor = u32::from_le_bytes(
            match bytes.next_n(4).as_slice().try_into() {
                Ok(b) => b,
                Err(e) => {
                    return Err(e.to_string());
                }
            }
        );

        Ok(FileMetadata{
            parent_path: full_path.parent().unwrap_or(&PathBuf::new()).to_string_lossy().to_string(),
            name: full_path.file_name().unwrap_or(OsString::new().as_os_str()).to_string_lossy().to_string(),
            size: Some(filesize),
            file_type,
            uid,
            gid,
            mtime,
            #[cfg(unix)] mode: Some(mode), #[cfg(not(unix))] None,
            link_name: Some(link_name),
            #[cfg(unix)] dev_major: Some(dev_major), #[cfg(not(unix))] None,
            #[cfg(unix)] dev_minor: Some(dev_minor), #[cfg(not(unix))] None,
        })
    }

    fn deserialize_file_bytes(bytes: &mut Bytes, pb: &mut ProgressLogger) -> Result<Arc<Mutex<ArchiveEntry>>, String> {
        //read in metadata
        let metadata = Self::deserialize_metadata(bytes)?;

        //the next n bytes can be read in for this specific file
        if metadata.file_type == FileType::Directory {
            let mut dir = FolderArchive::with_metadata(metadata);

            let next_pos = bytes.get_read_position() + dir.metadata.size.unwrap_or(0) as usize;

            while bytes.get_read_position() < next_pos {
                pb.set_progress(bytes.get_read_position() as u64);
                dir.contents.push(Self::deserialize_file_bytes(bytes, pb)?);
            }
            pb.set_progress(bytes.get_read_position() as u64);

            Ok( Arc::new(Mutex::new( Folder( dir ) )) )

        } else {
            let mut file = FileArchive::new(metadata);
            let fbytes = bytes.next_n(file.metadata.size.unwrap_or(0) as usize);
            file.bytes = fbytes;
            pb.set_progress(bytes.get_read_position() as u64);

            Ok( Arc::new(Mutex::new( ArchiveEntry::File( file ) )) )
        }
    }

    pub fn deserialize(mut self) -> Result<FolderArchive, String> {
        let mut res = FolderArchive::container();

        let mut bar = ProgressLogger::new(self.bytes.len() as u64).unwrap();

        self.bytes.set_read_position(HEADER_SIZE);

        bar.set_progress(self.bytes.get_read_position() as u64);

        //time to read in the data, wooooooo
        loop {
            let section_bytes = self.bytes.next_n(4);
            let section = match str::from_utf8(section_bytes.as_slice()) {
                Ok(s) => s,
                Err(e) => {
                    return Err(e.to_string())
                }
            };


            bar.println(format!("Parsing section \"{}\"", section)).ignore();

            match section {
                "DATA" => {
                    //read in data
                    let data_size = u64::from_le_bytes(
                        match self.bytes.next_n(8).as_slice().try_into() {
                            Ok(b) => b,
                            Err(e) => {
                                return Err(e.to_string());
                            }
                        }
                    );

                    let next_pos = self.bytes.get_read_position() + data_size as usize;

                    while self.bytes.get_read_position() < next_pos {
                        bar.set_progress(self.bytes.get_read_position() as u64);

                        res.contents.push(Self::deserialize_file_bytes(&mut self.bytes, &mut bar)?)
                    }

                }
                _ => {
                    return Err(format!("Unknown section {}, possibly corrupted file.", section))
                }
            }


            bar.set_progress(self.bytes.get_read_position() as u64);

            if self.bytes.get_read_position() >= self.bytes.len() {
                break;
            }
        }

        if res.contents.len() == 1 {
            let rc = res.contents.pop().unwrap();
            let is_folder = rc.lock().unwrap().is_folder();

            if is_folder {
                res = match match match Arc::try_unwrap(rc) {
                    Ok(m) => m,
                    Err(_) => {

                        return Ok(res)                        
                    }
                }.into_inner() {
                    Ok(r) => r,
                    Err(_) => {
                        
                        return Ok(res)
                    }
                } {
                    Folder(fa) => fa,
                    ArchiveEntry::File(_) => {

                        return Ok(res)
                    }
                };
            }
        }

        bar.finalize().ignore();

        Ok(res)
    }
}