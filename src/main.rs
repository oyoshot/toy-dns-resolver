use crate::libs::lookup_domain;

mod libs;

fn main() {
    println!("example.com -> {:?}", lookup_domain("example.com"));
    println!("recurse.com -> {:?}", lookup_domain("recurse.com"));
    println!("metafilter.com -> {:?}", lookup_domain("metafilter.com"));
}
