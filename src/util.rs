use log::{LevelFilter};

pub fn parse_log_verbosity<'a>(verbosity: &'a str, module_path: &'a str) -> Vec<(&'a str, LevelFilter)> {
    let mut ret: Vec<(&str, LevelFilter)> = Vec::new();
    for module_level_str in verbosity.split(',') {
        let module_level: Vec<_> = module_level_str.split('%').collect();
        // Using `get(0)` looks more consistent along with the following `get(1)`
        #[allow(clippy::get_first)]
        let name = *module_level.get(0).unwrap_or(&".");
        let level = *module_level.get(1).unwrap_or(&"D");
        let module = if name == "." { module_path } else { name };
        let level = match level {
            "E" => LevelFilter::Error,
            "W" => LevelFilter::Warn,
            "I" => LevelFilter::Info,
            "D" => LevelFilter::Debug,
            _ => LevelFilter::Trace,
        };
        ret.push((module, level));
    }
    ret
}

pub fn hex_array(data: &[u8]) -> String {
    let mut ret = "[".to_string();
    for b in data {
        if ret.len() > 1 {
            ret += ",";
        }
        ret += &format!("0x{:02x}", b);
    }
    ret += "]";
    ret
}
pub fn hex_dump(data: &[u8]) -> String {
    let mut ret: String = Default::default();
    let mut hex_line: String = Default::default();
    let mut char_line: String = Default::default();
    let box_size = (data.len() / 16 + 1) * 16 + 1;
    for i in 0..box_size {
        let byte = if i < data.len() { Some(data[i]) } else { None };
        if i % 16 == 0 {
            ret += &hex_line;
            ret += &char_line;
            if byte.is_some() {
                if i > 0 {
                    ret += "\n";
                }
                ret += &format!("{:04x} ", i);
            }
            hex_line.clear();
            char_line.clear();
        }
        let hex_str = match byte {
            None => { "   ".to_string() }
            Some(b) => { format!("{:02x} ", b) }
        };
        let c_str = match byte {
            None => { " ".to_string() }
            Some(b) => {
                let c = b as char;
                let c = if c >= ' ' && c < (127 as char) { c } else { '.' };
                format!("{}", c)
            }
        };
        hex_line += &hex_str;
        char_line += &c_str;
    }
    ret
}


