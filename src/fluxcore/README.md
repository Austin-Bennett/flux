# Fluxcore
A library providing similar functionality to tar for rust

## Installation
```bash
cargo install fluxcore
```

## Basic Usage

```rust
use std::path::PathBuf;
use fluxcore::archive::FolderArchive;
use fluxcore::serialization::FluxFile;

pub fn archive_stuff() -> Option<()> {
    let mut my_archive = FolderArchive::new();
    my_archive.add(&PathBuf::from("/dir/test.txt")); //must be relative to the CWD
    my_archive.add(&PathBuf::from("/to_backup/")); //adds the whole folder recursively
    
    //output to a flux archive
    let serialized = FluxFile::serialize(my_archive, true).unwrap();
    serialized.save("archive.flux");
    
    ...
    
    
    let mut archive = FluxFile::load("archive.flux").unwrap().deserialize().unwrap();
    archive.output_directory(); //loads contents into the CWD
}
```

