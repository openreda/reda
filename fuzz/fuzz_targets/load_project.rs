//! Fuzzes Project loading: the migration chain, strict typed deserialisation and the
//! Project/Document consistency checks must reject bad input with an error, never a panic.
//!
//! Input layout: the first two bytes (little-endian) give the length of the Project file;
//! the rest of the input is served for every Document file the Project file names. A Project
//! that loads must then save, load and save again to the same bytes.

#![no_main]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use libfuzzer_sys::fuzz_target;
use reda_engine::format::LoadError;
use reda_engine::store::{encode, load_from};

fuzz_target!(|data: &[u8]| {
    let Some((len, rest)) = data.split_first_chunk::<2>() else {
        return;
    };
    let split = usize::from(u16::from_le_bytes(*len)).min(rest.len());
    let (project_bytes, document_bytes) = rest.split_at(split);
    let project_file = PathBuf::from("Demo/Demo.reda");

    let loaded = load_from(&project_file, |path: &Path| {
        Ok(if path == project_file {
            project_bytes.to_vec()
        } else {
            document_bytes.to_vec()
        })
    });
    let Ok((project, _)) = loaded else {
        return;
    };

    let files: BTreeMap<String, Vec<u8>> = encode(&project)
        .expect("a loaded Project encodes")
        .into_iter()
        .collect();
    let (again, _) = load_from(&project_file, |path: &Path| {
        let rel = path.strip_prefix("Demo").unwrap().to_str().unwrap();
        files
            .get(rel)
            .cloned()
            .ok_or_else(|| LoadError::Missing(path.to_owned()))
    })
    .expect("a saved Project loads");
    assert_eq!(again, project);
    let resaved: BTreeMap<String, Vec<u8>> = encode(&again).unwrap().into_iter().collect();
    assert_eq!(resaved, files);
});
