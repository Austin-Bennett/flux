use crate::logging::log;
use std::fs::File;
use crate::logging::err_log;
use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::{io, ptr};
use std::io::Write;
use crate::archive::{ArchiveEntry, FileArchive, FileMetadata, FolderArchive};
use crate::{err_log, log};

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
    None = 0
}

impl FluxFile {

    pub fn save(&self, filename: impl AsRef<Path>) -> io::Result<()> {
        let mut file = File::create(filename)?;

        file.write_all(self.bytes.as_slice())
    }

    pub fn serialize(archive: FolderArchive, compression_mode: CompressionMode) -> Option<FluxFile> {
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

        //output the data bytes
        bytes.extend_from_slice("DATA".as_bytes());
        //output the size of the data
        bytes.raw_bytes(&(data.len() as u64));
        //finally, output the data
        bytes.extend(data);

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

        log!(true, "Serializing metadata for folder [content path = {}]", fullpath);
        //once we parse we have to change the size stored here
        let mut smetadata = Self::serialize_metadata(&archive.metadata);



        let mut data = Bytes::new();

        //collect data
        for c in &archive.contents {
            log!(true, "Serializing entry for folder [content path = {}]", fullpath);
            let lock = c.lock().unwrap();
            let bytes = match lock.deref() {
                ArchiveEntry::File(fa) => Self::serialize_file(fa),
                ArchiveEntry::Folder(fa) => Self::serialize_folder(fa, depth+1)?
            };

            data.extend(bytes)
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
}