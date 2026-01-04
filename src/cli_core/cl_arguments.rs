/*
usage:
NOTE: files/folders can also be specified by a regex string rather than just plaintext
flux file1 file2 ...                                        #archives all files into archive.flux archive
flux file1 file2 ... -o name.whatever                       #archives all files into one .flux archive with the specified file (or specify the only the name for the default .flux or use \. (ex: -o name >> name.flux -o name\.archive >> name.archive.flux)
flux -u [--unarchive] archive.flux                          #unarchives the archive into the CWD
flux -u -o [--output] /path/to/output/directory             #unarchives the archive into the output directory
flux -a [--add] archive.flux file1 file2 ...                #adds the files to an existing archive
flux -r [--rip] archive.flux file1 file2                    #unarchives the specified files from the archive, removing them altogether (ripping)
flux -r [--rip] archive.flux file1 file2 -o /output/dir     #rips the specified files into the output dir
flux -e [--extract] archive.flux file1 file2                #unarchives the specified files from the archive without removing them (extracting)
flux -e [--extract] archive.flux file1 file2 -o /output/dir #extracts the specified files into the output directory
flux -l [--list] archive.flux                               #lists all contained files and folders inside the archive
flux -f [--find] archive.flux pattern1.* pattern2.*         #searches in the flux archive for the specified files matching the regex pattern and lists them
== other options ==
-v [--verbose] #enables more verbose logging to handle errors better
-h [--help]    #to show this prompt again
--compress # compresses the data
*/
use std::fmt::Display;
use clap::Parser;

pub enum CLIOperation {
    Archive,
    Unarchive,
    Add,
    Rip,
    Extract,
    List,
    Find
}

impl Display for CLIOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            CLIOperation::Archive => String::from("ARCHIVE"),
            CLIOperation::Unarchive => String::from("UNARCHIVE"),
            CLIOperation::Add => String::from("ADD"),
            CLIOperation::Rip => String::from("RIP"),
            CLIOperation::Extract => String::from("EXTRACT"),
            CLIOperation::List => String::from("LIST"),
            CLIOperation::Find => String::from("FIND")
        };
        write!(f, "{}", str)
    }
}


#[derive(Parser)]
#[command(disable_help_flag = true)]
pub struct Arguments {
    /* flags */
    #[arg(short, long, default_value = "false")]
    pub verbose: bool,

    #[arg(short, long, default_value = "false")]
    pub help: bool,

    /* parameters */

    #[arg(short, long, default_value = "\0")]
    pub unarchive: String,

    #[arg(short, long, default_value = "\0")]
    pub add: String,

    #[arg(short, long, default_value = "\0")]
    pub rip: String,

    #[arg(short, long, default_value = "\0")]
    pub extract: String,

    #[arg(short, long, default_value = "\0")]
    pub list: String,

    #[arg(short, long, default_value = "\0")]
    pub find: String,

    #[arg(short, long, default_value = "\0")]
    pub output: String,

    #[arg(long, default_value = "true")]
    pub compress: bool,

    pub patterns: Vec<String> //contains files/patterns
}

impl Arguments {
    pub fn get_operation(&self) -> CLIOperation {
        if self.unarchive != "\0" { CLIOperation::Unarchive }
        else if self.add != "\0" { CLIOperation::Add }
        else if self.rip != "\0" { CLIOperation::Rip }
        else if self.extract != "\0" { CLIOperation::Extract }
        else if self.list != "\0" { CLIOperation::List }
        else if self.find != "\0" { CLIOperation::Find }
        else { CLIOperation::Archive }
    }
}