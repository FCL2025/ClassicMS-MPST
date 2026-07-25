//! WZ 資產 MonoBehaviour 與 MonoScript / external 關聯偵查。
//!
//! 用法：
//! `cargo run --release --features research-tools --example wzss_meta -- <bundle 目錄> <spritesheet stem> <logical path>`

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use msclassic::unity::bundle::Bundle;
use msclassic::unity::objects;
use msclassic::unity::serialized_file::{metadata_len, SerializedFile, HEADER_SIZE};

fn serialized_file(
    bundle: &Bundle,
    node_index: usize,
) -> Result<SerializedFile, Box<dyn std::error::Error>> {
    let head48 = bundle.read_node_range(node_index, 0, HEADER_SIZE)?;
    let meta_len = metadata_len(&head48)?;
    let head = bundle.read_node_range(node_index, 0, meta_len)?;
    Ok(SerializedFile::parse(&head)?)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("用法: wzss_meta <bundle 目錄> <spritesheet stem> <logical path>".into());
    }
    let dir = PathBuf::from(&args[0]);
    let sheet = Bundle::open(&dir.join(format!("{}.bundle", args[1])))?;
    let needle = args[2].to_ascii_lowercase();
    let mut target_script_ids = HashSet::new();

    for (node_index, node) in sheet.nodes.iter().enumerate() {
        if !node.is_serialized_file() {
            continue;
        }
        let sf = serialized_file(&sheet, node_index)?;
        println!("== sheet node {} ==", node.name);
        for (index, external) in sf.externals.iter().enumerate() {
            println!(
                "external fileID={} guid={} type={} path={}",
                index + 1,
                external
                    .guid
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>(),
                external.type_,
                external.path_name
            );
        }
        let by_path_id: HashMap<_, _> = sf
            .objects
            .iter()
            .map(|object| (object.path_id, object))
            .collect();
        for bundle_object in sf.objects_by_class(objects::CLASS_ASSET_BUNDLE) {
            let data = sheet.read_node_range(
                node_index,
                sf.data_offset + bundle_object.byte_start as u64,
                bundle_object.byte_size as usize,
            )?;
            let asset_bundle = objects::parse_asset_bundle(&data, &sf)?;
            for entry in &asset_bundle.container {
                let lower = entry.path.to_ascii_lowercase();
                if !lower.contains(&needle) {
                    continue;
                }
                let Some(object) = by_path_id.get(&entry.asset.path_id) else {
                    continue;
                };
                let data = sheet.read_node_range(
                    node_index,
                    sf.data_offset + object.byte_start as u64,
                    object.byte_size as usize,
                )?;
                let mono = objects::parse_mono_behaviour(&data, &sf)?;
                println!(
                    "WZ asset {} pid={} class={} script=(fileID={}, pathID={}) name={:?} payload={}",
                    entry.path,
                    object.path_id,
                    object.class_id,
                    mono.script.file_id,
                    mono.script.path_id,
                    mono.name,
                    data.len().saturating_sub(mono.header_len)
                );
                target_script_ids.insert(mono.script.path_id);
            }
        }
    }

    for path in std::fs::read_dir(&dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_stem()
                .is_some_and(|stem| stem.to_string_lossy().starts_with("monoscripts_"))
        })
    {
        let scripts = Bundle::open(&path)?;
        println!("== {} ==", path.display());
        for (node_index, node) in scripts.nodes.iter().enumerate() {
            if !node.is_serialized_file() {
                continue;
            }
            let sf = serialized_file(&scripts, node_index)?;
            for object in sf.objects_by_class(objects::CLASS_MONO_SCRIPT) {
                let data = scripts.read_node_range(
                    node_index,
                    sf.data_offset + object.byte_start as u64,
                    object.byte_size as usize,
                )?;
                let script = objects::parse_mono_script(&data, &sf)?;
                if !target_script_ids.contains(&object.path_id)
                    && !script.class_name.eq_ignore_ascii_case(&needle)
                {
                    continue;
                }
                println!(
                    "MonoScript pid={} name={:?} class={:?} namespace={:?} assembly={:?}",
                    object.path_id,
                    script.name,
                    script.class_name,
                    script.namespace,
                    script.assembly_name
                );
            }
        }
    }
    Ok(())
}
