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
use std::path::PathBuf;
use clap::{Command, Parser};
use crate::cli_core::CLIOperation;
use flux_lib;
use flux_lib::log;
use flux_lib::logging::log;
use crate::cli_core::CLIOperation::Archive;

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
        CLIOperation::Archive => {
            let mut archive = flux_lib::archive::Archive::new();
            for f in &arguments.patterns {
                //todo: turn patterns into compiled regex first
                archive.add_file(&PathBuf::from(f));
            }
            println!("{}", archive.to_string());
        }
        CLIOperation::Unarchive => {
            todo!()
        }
        CLIOperation::Add => {
            todo!()
        }
        CLIOperation::Rip => {
            todo!()
        }
        CLIOperation::Extract => {
            todo!()
        }
        CLIOperation::List => {
            todo!()
        }
        CLIOperation::Find => {
            todo!()
        }
    }
}
