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
use crate::flux_lib::logging::fatal_log;
use crate::flux_lib::logging::err_log;
use std::path::PathBuf;
use std::process::exit;
use clap::{Command, Parser};
use crate::cli_core::CLIOperation;
use flux_lib;
use flux_lib::archive::FolderArchive;
use flux_lib::{err_log, fatal_log, log};
use flux_lib::logging::log;
use flux_lib::serialization::{CompressionMode, FluxFile};
use crate::cli_core::CLIOperation::*;

mod cli_core;

fn main() {
    let arguments = cli_core::Arguments::parse();

    if arguments.verbose {
        flux_lib::logging::enable_verbose_logging();
        log!(false, "Verbose logging enabled");
    }

    // log(format!("Operation mode: {}", arguments.get_operation()).as_str(), false);
    log!(false, "Operation mode: {}", arguments.get_operation());

    match arguments.get_operation() {
        Archive => {
            //todo: loading bars
            let mut archive = FolderArchive::container();

            for file in arguments.patterns {
                match archive.add(&PathBuf::from(file)) {
                    Some(_) => {},
                    None => {
                        panic!("Archive operation failed!");
                    }
                }
            }
            if arguments.verbose {
                println!("=== file structure ===\n{}", archive);
            }
            log!(false, "Outputting archive...");
            let flux_archive = FluxFile::serialize(archive, CompressionMode::None);
            match flux_archive.unwrap().save(arguments.output) {
                Ok(()) => {},
                Err(e) => {
                    fatal_log!("Failed to save archive due to error: {}", e.to_string());
                }
            }
            log!(false, "Successfully created archive!");
        }
        Unarchive => {
            todo!()
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
