use std::fs::File;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::sync::atomic::AtomicBool;

use anyhow::Result;
use gix::ObjectId;
use gix::actor::SignatureRef;
use gix::parallel::InOrderIter;
use gix_pack::data::output::bytes::FromEntriesIter;
use gix_pack::data::output::count::objects;
use gix_pack::data::output::count::objects::ObjectExpansion;
use gix_pack::data::output::count::objects::Options;
use gix_pack::data::output::entry::iter_from_counts;
use tar::Archive;
use tar::EntryType;
use tempfile::tempdir;

/// Calculate a 4 byte hex prefix and prepend it to the string content, and
/// turn it into bytes.
///
/// https://git-scm.com/docs/protocol-v2
pub fn ptk_bytes(data: &str) -> Vec<u8> {
    ptk_str(data).into_bytes()
}

pub fn ptk_str(data: &str) -> String {
    let len = data.len() + 4;
    format!("{:04x}{}", len, data)
}

/// Take a tarball and create a git repository with a single commit containing the contents of the
/// tarball. Take this repo and create a git-upload-pack file and a info/refs file suitable for mocking a
/// response to `git clone`. Return these values.
///
/// These response values are formatted in such a way that they can be sent directly across the
/// wire.
///
/// This function assumes the tarball is somewhat trusted (see onyx_api::storage::validate_tarball)
///
/// Returns, `(commit_hash, pack_bytes)`. The pack_bytes are ready to be sent over the wire to a
/// git client. The commit_hash is meant to be used in a dynamically constructed refs listing.
pub fn extract_git_mock(tarball: &mut File, version_name: &str) -> Result<(String, Vec<u8>)> {
    tarball.seek(SeekFrom::Start(0))?;

    let mut archive = Archive::new(tarball);
    let git_dir = tempdir()?;

    // TODO: make sure user git configurations aren't being read here or doing nasty things
    let repo = gix::init(&git_dir)?;
    let mut editor = repo.edit_tree(ObjectId::empty_tree(gix::hash::Kind::Sha1))?;
    for entry in archive.entries()? {
        let mut entry = entry?;
        match entry.header().entry_type() {
            EntryType::Regular => {
                let path = entry.path()?.to_path_buf();
                let mut bytes = Vec::default();
                entry.read_to_end(&mut bytes)?;
                let oid = repo.write_blob(bytes)?;
                editor.upsert(
                    path.to_string_lossy().to_string(),
                    gix::objs::tree::EntryKind::Blob,
                    oid,
                )?;
            }
            EntryType::Directory => {
                continue;
            }
            _ => anyhow::bail!(
                "Irregular entry detected in tar archive. Only directories and files are allowed in package tarballs!"
            ),
        }
    }

    let tree_id = editor.write()?;
    let commit_id = repo.commit_as(
        SignatureRef::default(),
        SignatureRef::default(),
        "HEAD",
        "default package commit",
        tree_id,
        Vec::<ObjectId>::default(),
    )?;

    // create the only branch
    repo.reference(
        format!("refs/heads/{version_name}"),
        commit_id,
        gix::refs::transaction::PreviousValue::MustNotExist,
        "create main branch",
    )?;

    let mut handle = repo.objects.store().to_handle();
    handle.prevent_pack_unload();

    // now our repo has a commit, let's build a git-upload-pack and a ref list to statically serve

    let (counts, outcome) = objects(
        handle,
        Box::new(vec![Ok(ObjectId::from(commit_id))].into_iter()),
        &gix::features::progress::Discard,
        &AtomicBool::new(false),
        Options {
            input_object_expansion: ObjectExpansion::TreeContents,
            ..Default::default()
        },
    )?;
    let mut handle = repo.objects.store().to_handle();
    handle.prevent_pack_unload();
    let pack_iter = iter_from_counts(
        counts,
        handle,
        Box::new(gix::features::progress::Discard),
        Default::default(),
    );
    let mut pack_bytes = Vec::new();
    // exhaust the iterator to finish packing
    for entry in FromEntriesIter::new(
        InOrderIter::from(pack_iter),
        // file,
        &mut pack_bytes,
        outcome.total_objects as u32,
        gix_pack::data::Version::V2,
        gix::hash::Kind::Sha1,
    ) {
        entry?;
    }

    let commit_hex = commit_id.to_hex().to_string();

    Ok((commit_hex, pack_bytes))
}
