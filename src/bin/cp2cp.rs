use std::{process, io, fs};
use std::io::{BufReader, BufRead, BufWriter, stdout};
use std::path::PathBuf;
use std::process::ExitCode;
use log::LevelFilter;
use shvproto::{ChainPackReader, ChainPackWriter, CponReader, CponWriter, ReadError};
use simple_logger::SimpleLogger;
use shvproto::Reader;
use shvproto::Writer;
use clap::{Parser};
use shvproto::reader::ReadErrorReason;

#[derive(Parser, Debug)]
#[structopt(name = "cp2cp", version = env!("CARGO_PKG_VERSION"), author = env!("CARGO_PKG_AUTHORS"), about = "ChainPack to Cpon and back utility")]
struct Cli {
    #[arg(short, long, help = "Cpon indentation string")]
    indent: Option<String>,
    #[arg(long = "ip", help = "Cpon input")]
    cpon_input: bool,
    #[arg(long = "oc", help = "ChainPack output")]
    chainpack_output: bool,
    /// Expect input data in RPC block transport format, https://silicon-heaven.github.io/shv-doc/rpctransportlayer/stream.html
    #[arg(long)]
    chainpack_rpc_block: bool,
    /// Verbose mode (module, .)
    #[arg(short = 'v', long = "verbose")]
    verbose: Option<String>,
    /// File to process
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,
}

const CODE_READ_ERROR: i32 = 1;
const CODE_NOT_ENOUGH_DATA: i32 = 2;
const CODE_WRITE_ERROR: i32 = 4;

fn process_read_error(error: &ReadError) -> ! {
    match error.reason {
        ReadErrorReason::UnexpectedEndOfStream => {
            process::exit(CODE_NOT_ENOUGH_DATA);
        }
        ReadErrorReason::InvalidCharacter => {
            eprintln!("Parse input error: {:?}", error);
            process::exit(CODE_READ_ERROR);
        }
    }
}
fn main() -> ExitCode {
    // Parse command line arguments
    let mut opts = Cli::parse();

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

    if opts.chainpack_rpc_block {
        opts.indent = None;
        opts.chainpack_output = false;
        opts.cpon_input = false;
    }

    let mut reader: Box<dyn BufRead> = match opts.file {
        None => Box::new(BufReader::new(io::stdin())),
        Some(filename) => Box::new(BufReader::new(fs::File::open(filename).unwrap()))
    };

    let mut chainpack_rpc_block_header = None;
    let res = if opts.cpon_input {
        let mut rd = CponReader::new(&mut reader);
        rd.read()
    } else {
        let mut rd = ChainPackReader::new(&mut reader);
        if opts.chainpack_rpc_block {
            let (block_length, frame_length) = match rd.read_uint_data() {
                Ok(frame_length) => { (frame_length as usize + rd.position(), frame_length) }
                Err(e) => { process_read_error(&e); }
            };
            let proto = match rd.read() {
                Ok(proto) => { proto.as_int() }
                Err(e) => { process_read_error(&e); }
            };
            chainpack_rpc_block_header = Some((block_length, frame_length, proto));
        }
        rd.read()
    };
    let rv = match res {
        Err(e) => { process_read_error(&e); }
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
        if let Some((block_length, frame_length, proto)) = chainpack_rpc_block_header {
            println!("{block_length}");
            println!("{frame_length}");
            println!("{proto}");
        }
        wr.write(&rv)
    };
    if let Err(e) = res {
        eprintln!("Write output error: {:?}", e);
        process::exit(CODE_WRITE_ERROR);
    };
    ExitCode::SUCCESS
}

