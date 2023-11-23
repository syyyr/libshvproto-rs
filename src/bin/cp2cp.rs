use std::{process, io, fs};
use std::io::{BufReader, BufRead, BufWriter, stdout};
use std::path::PathBuf;
use log::LevelFilter;
use structopt::StructOpt;
use shv::{ChainPackReader, ChainPackWriter, CponReader, CponWriter};
use simple_logger::SimpleLogger;
use shv::Reader;
use shv::Writer;

#[derive(StructOpt, Debug)]
#[structopt(name = "cp2cp", version = env!("CARGO_PKG_VERSION"), author = env!("CARGO_PKG_AUTHORS"), about = "ChainPack to Cpon and back utility")]
struct Cli {
    #[structopt(short, long, help = "Cpon indentation string")]
    indent: Option<String>,
    #[structopt(long = "--ip", help = "Cpon input")]
    cpon_input: bool,
    #[structopt(long = "--oc", help = "ChainPack output")]
    chainpack_output: bool,
    /// Verbose mode (module, .)
    #[structopt(short = "v", long = "verbose")]
    verbose: Option<String>,
    /// File to process
    #[structopt(name = "FILE", parse(from_os_str))]
    file: Option<PathBuf>,
}

fn main() {
    // Parse command line arguments
    let opts = Cli::from_args();

    let mut logger = SimpleLogger::new();
    logger = logger.with_level(LevelFilter::Error);
    if let Some(module_names) = &opts.verbose {
        for module_name in module_names.split(',') {
            let module_name = if module_name == "." {
                module_path!().to_string()
            } else {
                module_name.to_string()
            };
            logger = logger.with_module_level(&module_name, LevelFilter::Trace);
        }
    }
    logger.init().unwrap();

    //log::info!("=====================================================");
    //log::info!("{} starting up!", std::module_path!());
    //log::info!("=====================================================");
    //log::info!("Verbosity levels: {}", verbosity_string);

    let mut reader: Box<dyn BufRead> = match opts.file {
        None => Box::new(BufReader::new(io::stdin())),
        Some(filename) => Box::new(BufReader::new(fs::File::open(filename).unwrap()))
    };

    let res = if opts.cpon_input {
        let mut rd = CponReader::new(&mut reader);
        rd.read()
    } else {
        let mut rd = ChainPackReader::new(&mut reader);
        rd.read()
    };
    let rv = match res {
        Err(e) => {
            eprintln!("Parse input error: {:?}", e);
            process::exit(1);
        }
        Ok(rv) => rv,
    };
    let mut writer = BufWriter::new(stdout());
    let res = if opts.chainpack_output {
        let mut wr = ChainPackWriter::new(&mut writer);
        wr.write(&rv)
    } else {
        let mut wr = CponWriter::new(&mut writer);
        if let Some(s) = opts.indent {
            if s == "\\t" {
                wr.set_indent("\t".as_bytes());
            } else {
                wr.set_indent(s.as_bytes());
            }
        }
        wr.write(&rv)
    };
    if let Err(e) = res {
        eprintln!("Write output error: {:?}", e);
        process::exit(1);
    }
}

