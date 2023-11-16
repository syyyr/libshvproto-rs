use lazy_static::lazy_static;
use shv::metamethod::MetaMethod;
use shv::shvnode::ShvNode;

lazy_static! {
    static ref DIR_LS: [MetaMethod; 2] = [
        MetaMethod { name: "dir".into(), param: "DirParam".into(), result: "DirResult".into(), ..Default::default() },
        MetaMethod { name: "ls".into(), param: "LsParam".into(), result: "LsResult".into(), ..Default::default() },
    ];
}

struct DirNode {
    // methods: Vec<MetaMethod>
}
impl<'a> ShvNode<'a> for DirNode {
    type MethodIterator = std::slice::Iter<'a, MetaMethod>;
    fn methods(&'a self) -> Self::MethodIterator {
        DIR_LS.iter()
    }
}

lazy_static! {
    static ref APP_METHODS: [MetaMethod; 1] = [
        MetaMethod { name: "ping".into(), ..Default::default() },
    ];
}
struct AppNode {
}
impl<'a> ShvNode<'a> for AppNode {
    type MethodIterator = std::iter::Chain<std::slice::Iter<'a, MetaMethod>, std::slice::Iter<'a, MetaMethod>>;
    fn methods(&'a self) -> Self::MethodIterator {
        DIR_LS.iter().chain(APP_METHODS.iter())
    }
}