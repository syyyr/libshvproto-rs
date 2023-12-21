use log::LevelFilter;
use sha1::Sha1;
use sha1::Digest;

pub fn sha1_hash(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha1::new();
    hasher.update(data);
    let result = hasher.finalize();
    return hex::encode(&result[..]).as_bytes().to_vec();
}
pub fn sha1_password_hash(password: &[u8], nonce: &[u8]) -> Vec<u8> {
    let mut hash = sha1_hash(password);
    let mut nonce_pass= nonce.to_vec();
    nonce_pass.append(&mut hash);
    return sha1_hash(&nonce_pass);
}
pub fn join_path(p1: &str, p2: &str) -> String {
    if p1.is_empty() && p2.is_empty() {
        "".to_string()
    } else if p1.is_empty() {
        p2.to_string()
    } else if p2.is_empty() {
        p1.to_string()
    } else {
        p1.to_string() + "/" + p2
    }
}

pub fn parse_log_verbosity<'a>(verbosity: &'a str, module_path: &'a str) -> Vec<(&'a str, LevelFilter)> {
    let mut ret: Vec<(&str, LevelFilter)> = Vec::new();
    for module_level_str in verbosity.split(',') {
        let module_level: Vec<_> = module_level_str.split(':').collect();
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