use clap::Parser;
use log::LevelFilter;
use shvproto::reader::ReadErrorReason;
use shvproto::Reader;
use shvproto::Writer;
use shvproto::{ChainPackReader, ChainPackWriter, CponReader, CponWriter};
use simple_logger::SimpleLogger;
use std::fmt::Display;
use std::io::{stdout, BufRead, BufReader, BufWriter};
use std::path::PathBuf;
use std::{fs, io, process};

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

const CODE_SUCCESS: i32 = 0;
const CODE_READ_ERROR: i32 = 1;
const CODE_NOT_ENOUGH_DATA: i32 = 2;
const CODE_WRITE_ERROR: i32 = 4;

struct ChainPackRpcBlockResult {
    block_length: Option<usize>,
    frame_length: Option<u64>,
    proto: Option<i64>,
    cpon: String,
}
fn print_option<T: Display>(n: Option<T>) {
    if let Some(n) = n {
        println!("{}", n);
    } else {
        println!();
    }
}
fn exit_with_result_and_code(result: &ChainPackRpcBlockResult, error: Option<ReadErrorReason>) -> ! {
    let exit_code = if let Some(error) = &error {
        match error {
            ReadErrorReason::UnexpectedEndOfStream => CODE_NOT_ENOUGH_DATA,
            ReadErrorReason::InvalidCharacter => {
                eprintln!("Parse input error: {:?}", error);
                CODE_READ_ERROR
            }
        }
    } else {
        CODE_SUCCESS
    };
    print_option(result.block_length);
    print_option(result.frame_length);
    print_option(result.proto);
    println!("{}", result.cpon);
    process::exit(exit_code);
}
fn process_chainpack_rpc_block(mut reader: Box<dyn BufRead>) -> ! {
    let mut result = ChainPackRpcBlockResult {
        block_length: None,
        frame_length: None,
        proto: None,
        cpon: "".to_string(),
    };
    let mut rd = ChainPackReader::new(&mut reader);
    match rd.read_uint_data() {
        Ok(frame_length) => {
            result.block_length = Some(frame_length as usize + rd.position());
            result.frame_length = Some(frame_length);
        }
        Err(e) => {
            exit_with_result_and_code(&result, Some(e.reason));
        }
    };
    match rd.read() {
        Ok(proto) => {
            let proto = proto.as_int();
            if proto == 0 || proto == 1 {
                result.proto = Some(proto);
            } else {
                exit_with_result_and_code(&result, Some(ReadErrorReason::InvalidCharacter));
            }
        }
        Err(e) => {
            exit_with_result_and_code(&result, Some(e.reason));
        }
    };
    match rd.read() {
        Ok(rv) => {
            result.cpon = rv.to_cpon().to_string();
            exit_with_result_and_code(&result, None);
        }
        Err(e) => {
            exit_with_result_and_code(&result, Some(e.reason));
        }
    }
}
fn main() {
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
        Some(filename) => Box::new(BufReader::new(fs::File::open(filename).unwrap())),
    };

    let res = if opts.cpon_input {
        let mut rd = CponReader::new(&mut reader);
        rd.read()
    } else if opts.chainpack_rpc_block {
        process_chainpack_rpc_block(reader)
    } else {
        let mut rd = ChainPackReader::new(&mut reader);
        rd.read()
    };
    let rv = match res {
        Err(e) => {
            eprintln!("Parse input error: {:?}", e);
            process::exit(CODE_READ_ERROR);
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
        process::exit(CODE_WRITE_ERROR);
    };
}
