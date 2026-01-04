/*
________________________________________________________________________
|============================================================== -- [] x|
|======================================================================|
|                                                                      |
|   ____________   ______       _____        _____ _____     /\        |
|   | ++        \  |    |       |   |        |   | \    \   /  \       |
|   | +  ________\ |    |       |   |        |   |  \    \ /  + \      |
|   |    |         |    |       |   |        |   |   \    \  + /       |
|   |    |________ |    |       |   |        |   |    \    \  /        |
|   |            / |    |       |   |        |   |     \    \/         |
|   |     ______/  |    |       |   |        |   |     /\    \         |
|   |    |         |    |       |   |        |   |    /  \    \        |
|   |    |         |    |       |   |        |   |   /    \    \       |
|   |    |         |    |       |   |        |   |  / +  / \    \      |
|   |    |         | +  |______ | +  \______/  + | / +  /   \    \     |
|   |    |         | ++        \ \ +    ++    + / / +  /     \    \    |
|   |____|         |____________\ \____________/ /____/       \____\   |
|________________________________________________CLI_tool______________|
*/
use crate::cli_core::CLIOperation::*;
use crate::fluxcore::logging::err_log;
use crate::fluxcore::logging::fatal_log;
use clap::Parser;
use fluxcore;
use fluxcore::archive::{FileMetadata, FolderArchive};
use fluxcore::logging::log;
use fluxcore::serialization::FluxFile;
use fluxcore::utils::ProgressTracker;
use fluxcore::{err_log, fatal_log, log};
use glob::glob;
use std::path::PathBuf;
use std::{env, fs};

mod cli_core;

fn main() {
    let mut arguments = cli_core::Arguments::parse();

    if arguments.verbose {
        fluxcore::logging::enable_verbose_logging();
        log!(false, "Verbose logging enabled");
    }

    // log(format!("Operation mode: {}", arguments.get_operation()).as_str(), false);
    log!(false, "Operation mode: {}", arguments.get_operation());


    match arguments.get_operation() {
        Archive => {

            //first load all the files according to the patterns vector
            let mut files: Vec<PathBuf> = Vec::new();



            for p in &arguments.patterns {
                for entry in match glob(&p) {
                    Ok(g) => g,
                    Err(_) => {
                        err_log!(false, "Bad glob pattern {}, skipping over it...", p);
                        continue;
                    }
                } {
                    match entry {
                        Ok(path) => {
                            let path =
                                PathBuf::from(path);
                            if path == PathBuf::from(".") {
                                //add all the files in the cwd instead
                                for f in fs::read_dir(env::current_dir().unwrap()).unwrap() {
                                    if let Ok(f) = f {
                                        files.push(f.path().strip_prefix(env::current_dir().unwrap()).unwrap().to_path_buf());
                                    }
                                }
                            } else if path == PathBuf::from("..") {
                                err_log!(false, "Cannot add files not relative to the CWD to an archive!\nnote: \
                                you can to a different directory and use [-a] to add another file, skipping...");
                            } else {
                                files.push(path);
                            }
                        },
                        Err(e) => {
                            err_log!(false, "Encountered error when iterating through glob entries for {}: {}\n\
                            skipping over it...",
                            p, e.to_string());
                            continue;
                        }
                    }
                }
            }



            let cur_metadata = match FileMetadata::retrieve(&PathBuf::from(env::current_dir().unwrap())) {
                Some(md) => md,
                None => {
                    fatal_log!("Failed to get CWD metadata");
                }
            };



            let mut archive = FolderArchive::with_metadata(
                cur_metadata
            );

            archive.metadata.parent_path = String::new();
            archive.is_container = true;

            //exit(0);
            let mut pt = ProgressTracker::new(files.len() as u64);

            for file in &files {
                let pathstr = file.to_str().unwrap_or("{invalid path}");


                log!(true, "Adding file {} to archive", pathstr);
                match archive.add(&file) {
                    Some(_) => {},
                    None => {
                        fatal_log!("Archive operation failed!");
                    }
                }
                pt.inc();
                println!("{}/{} ({:.1}%) files found", pt.cur(), pt.max(), pt.percent()*100.0);
            }


            log!(false, "Outputting archive...");
            let flux_archive = FluxFile::serialize(archive, arguments.compress);

            if arguments.output == "\0" {
                arguments.output = String::from("archive.flux");
            }

            match flux_archive.unwrap().save(arguments.output) {
                Ok(()) => {},
                Err(e) => {
                    fatal_log!("Failed to save archive due to error: {}", e.to_string());
                }
            }
            log!(false, "Successfully created archive!");
        }
        Unarchive => {
            let loc = PathBuf::from(arguments.unarchive);
            
            let archive = FluxFile::load(loc).unwrap().deserialize().unwrap();

            println!("Loaded archive: {}", archive);
            match archive.output_directory() {
                Ok(()) => {},
                Err(e) => {
                    fatal_log!("Failed to output full contents because of error: {}", e.to_string())
                }
            }
        }
        Add => {
            todo!()
        }
        Rip => {
            todo!()
        }
        Extract => {
            todo!()
        }
        List => {
            todo!()
        }
        Find => {
            todo!()
        }
    }
}
