use glob::Pattern;
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

pub fn glob_match(path: &str, pattern: &str) -> bool {
    Pattern::new(pattern).unwrap().matches(path)
}