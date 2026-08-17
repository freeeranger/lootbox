use std::{
    collections::HashMap,
    fs::{self, File},
    io::{BufReader, BufWriter, Read},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use rayon::prelude::*;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

use walkdir::{DirEntry, WalkDir};

use crate::{
    db::*,
    error::{LootboxError, Result},
    ingest::{classifier::*, textures::*},
    models::{ImportProgress, PackSummary},
    state::AppState,
};

pub fn should_visit(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let name = entry.file_name().to_string_lossy();
    if entry.file_type().is_dir() {
        !name.starts_with('.') && name != "node_modules" && name != "target"
    } else {
        !name.starts_with('.')
    }
}

pub fn modified_timestamp(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

pub fn hash_file(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn hash_unhashed_assets(state: &AppState) -> Result<usize> {
    if state.hashing_library.swap(true, Ordering::AcqRel) {
        return Ok(0);
    }
    let result = (|| {
        let connection = state.connect()?;
        let jobs = {
            let mut statement = connection.prepare(
                "SELECT id, absolute_path, size_bytes, modified_at FROM assets WHERE content_hash IS NULL AND missing = 0",
            )?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };
        let hashes: Vec<(i64, i64, i64, String)> = jobs
            .into_par_iter()
            .filter_map(|(id, path, size, modified)| {
                hash_file(Path::new(&path))
                    .ok()
                    .map(|hash| (id, size, modified, hash))
            })
            .collect();
        let _guard = state
            .write_queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut connection = state.connect()?;
        let transaction = connection.transaction()?;
        for (id, size, modified, hash) in &hashes {
            transaction.execute(
                "UPDATE assets SET content_hash = ?1 WHERE id = ?2 AND size_bytes = ?3 AND modified_at = ?4 AND missing = 0",
                params![hash, id, size, modified],
            )?;
        }
        transaction.commit()?;
        Ok(hashes.len())
    })();
    state.hashing_library.store(false, Ordering::Release);
    result
}

pub fn generate_thumbnail(source: &Path, destination: &Path) -> Option<()> {
    let file = File::open(source).ok()?;
    let reader = BufReader::new(file);
    let image = image::ImageReader::new(reader)
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    fs::create_dir_all(destination.parent()?).ok()?;
    let out_file = File::create(destination).ok()?;
    let mut writer = BufWriter::new(out_file);
    image
        .thumbnail(384, 288)
        .write_to(&mut writer, image::ImageFormat::Png)
        .ok()?;
    Some(())
}

pub fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

pub fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub fn rotate_log_if_needed(path: &Path) -> std::io::Result<()> {
    if fs::metadata(path).is_ok_and(|metadata| metadata.len() >= 2 * 1024 * 1024) {
        let rotated = path.with_extension("log.1");
        if rotated.is_file() {
            fs::remove_file(&rotated)?;
        }
        fs::rename(path, rotated)?;
    }
    Ok(())
}

pub fn import_pack_from_path(
    connection: &mut Connection,
    root: &Path,
    thumbnail_directory: Option<&Path>,
    cancelled: Option<&AtomicBool>,
    on_progress: &mut impl FnMut(ImportProgress),
) -> Result<PackSummary> {
    on_progress(ImportProgress {
        phase: "scanning",
        current: 0,
        total: 0,
        path: None,
    });
    let mut files = Vec::new();
    let mut last_progress = Instant::now();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(should_visit)
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        if cancelled.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            return Err(LootboxError::ImportCancelled);
        }
        files.push(entry.into_path());
        if files.len() % 250 == 0 || last_progress.elapsed() >= Duration::from_millis(100) {
            on_progress(ImportProgress {
                phase: "scanning",
                current: files.len(),
                total: 0,
                path: None,
            });
            last_progress = Instant::now();
        }
    }

    let total = files.len();
    on_progress(ImportProgress {
        phase: "hashing",
        current: 0,
        total,
        path: None,
    });
    let existing_hashes = {
        let mut statement = connection.prepare(
            "SELECT relative_path, size_bytes, modified_at, content_hash FROM assets WHERE pack_id = (SELECT id FROM packs WHERE root_path = ?1)",
        )?;
        let rows = statement
            .query_map(params![path_string(root)], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(path, size, modified, hash)| (path, (size, modified, hash)))
            .collect::<HashMap<_, _>>()
    };
    let mut content_hashes = HashMap::new();
    last_progress = Instant::now();
    for (index, absolute_path) in files.iter().enumerate() {
        if cancelled.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            return Err(LootboxError::ImportCancelled);
        }
        let relative_path = absolute_path.strip_prefix(root).unwrap_or(absolute_path);
        let relative_string = path_string(relative_path);
        let hash = fs::metadata(absolute_path).ok().and_then(|metadata| {
            let modified = modified_timestamp(&metadata);
            existing_hashes
                .get(&relative_string)
                .filter(|(size, previous_modified, hash)| {
                    *size == metadata.len() as i64
                        && *previous_modified == modified
                        && hash.is_some()
                })
                .and_then(|entry| entry.2.clone())
                .or_else(|| hash_file(absolute_path).ok())
        });
        if let Some(hash) = hash {
            content_hashes.insert(absolute_path.clone(), hash);
        }
        if index + 1 == total || last_progress.elapsed() >= Duration::from_millis(100) {
            on_progress(ImportProgress {
                phase: "hashing",
                current: index + 1,
                total,
                path: Some(relative_string),
            });
            last_progress = Instant::now();
        }
    }
    on_progress(ImportProgress {
        phase: "indexing",
        current: 0,
        total,
        path: None,
    });

    let pack_name = root
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Asset pack".to_string());
    let root_path = path_string(&root);
    let transaction = connection.transaction()?;
    let mut thumbnail_jobs = Vec::new();

    transaction.execute(
        r#"
        INSERT INTO packs(name, root_path, last_scanned_at, generation)
        VALUES (?1, ?2, CURRENT_TIMESTAMP, 1)
        ON CONFLICT(root_path) DO UPDATE SET
            last_scanned_at = CURRENT_TIMESTAMP,
            generation = packs.generation + 1
        "#,
        params![pack_name, root_path],
    )?;

    let (pack_id, generation): (i64, i64) = transaction.query_row(
        "SELECT id, generation FROM packs WHERE root_path = ?1",
        params![root_path],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    last_progress = Instant::now();
    for (index, absolute_path) in files.iter().enumerate() {
        if cancelled.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            return Err(LootboxError::ImportCancelled);
        }
        let relative_path = absolute_path.strip_prefix(&root).unwrap_or(absolute_path);
        let relative_path_string = path_string(relative_path);
        if index == 0 || last_progress.elapsed() >= Duration::from_millis(100) {
            on_progress(ImportProgress {
                phase: "indexing",
                current: index,
                total,
                path: Some(relative_path_string.clone()),
            });
            last_progress = Instant::now();
        }
        let metadata = match fs::metadata(absolute_path) {
            Ok(metadata) => metadata,
            Err(_) => {
                transaction.execute(
                    "UPDATE assets SET generation = ?1, missing = 1, missing_since = COALESCE(missing_since, CURRENT_TIMESTAMP) WHERE pack_id = ?2 AND relative_path = ?3",
                    params![generation, pack_id, relative_path_string],
                )?;
                continue;
            }
        };
        let extension = absolute_path
            .extension()
            .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let asset_type = classify_asset_type(relative_path, &extension);
        let name = absolute_path
            .file_stem()
            .or_else(|| absolute_path.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());
        let variant_group = model_variant_group(relative_path, asset_type, &extension);
        let (width, height) = image_dimensions(absolute_path, asset_type);
        let (triangles, vertices) = if asset_type == "model" {
            model_poly_count(absolute_path, &extension)
        } else {
            (None, None)
        };
        let modified_at = modified_timestamp(&metadata);
        let content_hash = content_hashes.get(absolute_path).cloned();
        let mut existing: Option<(i64, i64, i64, Option<String>, i64)> = transaction
            .query_row(
                "SELECT id, modified_at, size_bytes, thumbnail_path, thumbnail_version FROM assets WHERE pack_id = ?1 AND relative_path = ?2",
                params![pack_id, relative_path_string],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .optional()?;

        // A unique stale size/mtime/format match is a rename or move. Reuse its
        // row so tags, collections, exclusions and manual overrides survive.
        if existing.is_none() {
            let candidates = if let Some(hash) = &content_hash {
                let mut statement = transaction.prepare(
                    "SELECT id, modified_at, size_bytes, thumbnail_path, thumbnail_version FROM assets WHERE pack_id = ?1 AND generation != ?2 AND content_hash = ?3 AND extension = ?4 LIMIT 2",
                )?;
                let rows = statement
                    .query_map(params![pack_id, generation, hash, extension], |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                rows
            } else {
                let mut statement = transaction.prepare(
                    "SELECT id, modified_at, size_bytes, thumbnail_path, thumbnail_version FROM assets WHERE pack_id = ?1 AND generation != ?2 AND size_bytes = ?3 AND modified_at = ?4 AND extension = ?5 LIMIT 2",
                )?;
                let rows = statement
                    .query_map(
                        params![
                            pack_id,
                            generation,
                            metadata.len() as i64,
                            modified_at,
                            extension
                        ],
                        |row| {
                            Ok((
                                row.get(0)?,
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get(4)?,
                            ))
                        },
                    )?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                rows
            };
            if candidates.len() == 1 {
                existing = candidates.into_iter().next();
                transaction.execute(
                    "UPDATE assets SET relative_path = ?1 WHERE id = ?2",
                    params![relative_path_string, existing.as_ref().map(|entry| entry.0)],
                )?;
            }
        }

        transaction.execute(
            r#"
            INSERT INTO assets(
                pack_id, relative_path, absolute_path, name, extension, asset_type,
                size_bytes, modified_at, width, height, triangles, vertices, variant_group, content_hash, generation
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            ON CONFLICT(pack_id, relative_path) DO UPDATE SET
                absolute_path = excluded.absolute_path,
                name = excluded.name,
                extension = excluded.extension,
                asset_type = excluded.asset_type,
                size_bytes = excluded.size_bytes,
                modified_at = excluded.modified_at,
                width = excluded.width,
                height = excluded.height,
                triangles = excluded.triangles,
                vertices = excluded.vertices,
                variant_group = excluded.variant_group,
                content_hash = excluded.content_hash,
                generation = excluded.generation,
                missing = 0,
                missing_since = NULL
            "#,
            params![
                pack_id,
                relative_path_string,
                path_string(absolute_path),
                name,
                extension,
                asset_type,
                metadata.len() as i64,
                modified_at,
                width,
                height,
                triangles,
                vertices,
                variant_group,
                content_hash,
                generation,
            ],
        )?;

        if asset_type == "image" || asset_type == "texture" {
            if let Some(thumbnail_directory) = thumbnail_directory {
                let asset_id = existing
                    .as_ref()
                    .map(|entry| entry.0)
                    .unwrap_or_else(|| transaction.last_insert_rowid());
                let thumbnail_path = thumbnail_directory.join(format!("{asset_id}.png"));
                let thumbnail_is_current = existing.as_ref().is_some_and(|entry| {
                    entry.1 == modified_at
                        && entry.2 == metadata.len() as i64
                        && entry.4 == IMAGE_THUMBNAIL_VERSION
                        && entry
                            .3
                            .as_ref()
                            .is_some_and(|path| Path::new(path).is_file())
                });
                if thumbnail_is_current {
                    transaction.execute(
                        "UPDATE assets SET thumbnail_path = ?1, thumbnail_version = ?2 WHERE id = ?3",
                        params![path_string(&thumbnail_path), IMAGE_THUMBNAIL_VERSION, asset_id],
                    )?;
                } else {
                    transaction.execute(
                        "UPDATE assets SET thumbnail_path = NULL, thumbnail_version = 0 WHERE id = ?1",
                        params![asset_id],
                    )?;
                    thumbnail_jobs.push((asset_id, absolute_path.clone(), thumbnail_path));
                }
            }
        }

        if index + 1 == total {
            on_progress(ImportProgress {
                phase: "indexing",
                current: total,
                total,
                path: Some(path_string(relative_path)),
            });
        }
    }

    on_progress(ImportProgress {
        phase: "finalizing",
        current: total,
        total,
        path: None,
    });
    transaction.execute(
        "UPDATE assets SET missing = 1, missing_since = COALESCE(missing_since, CURRENT_TIMESTAMP) WHERE pack_id = ?1 AND generation != ?2",
        params![pack_id, generation],
    )?;
    recompute_texture_groups(&transaction, Some(pack_id))?;
    apply_classification_overrides(&transaction, Some(pack_id))?;
    recompute_primary_assets(&transaction, Some(pack_id))?;
    recompute_asset_dependencies(&transaction, Some(pack_id))?;
    transaction.commit()?;
    // Image decoding and resizing deliberately runs after the indexing
    // transaction, parallelized with Rayon across all CPU cores.
    let mut cancelled_after_commit = false;
    let results: Vec<(i64, PathBuf)> = thumbnail_jobs
        .into_par_iter()
        .filter_map(|(asset_id, source, destination)| {
            if cancelled.as_ref().is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                return None;
            }
            if generate_thumbnail(&source, &destination).is_some() {
                Some((asset_id, destination))
            } else {
                None
            }
        })
        .collect();

    if cancelled.as_ref().is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        cancelled_after_commit = true;
    }

    if !results.is_empty() {
        let tx = connection.transaction()?;
        {
            let mut stmt = tx.prepare(
                "UPDATE assets SET thumbnail_path = ?1, thumbnail_version = ?2 WHERE id = ?3",
            )?;
            for (asset_id, destination) in results {
                stmt.execute(params![path_string(&destination), IMAGE_THUMBNAIL_VERSION, asset_id])?;
            }
        }
        tx.commit()?;
    }
    rebuild_search_index(connection)?;
    let pack = get_pack(connection, pack_id)?;
    if cancelled_after_commit {
        return Err(LootboxError::ImportCancelled);
    }
    on_progress(ImportProgress {
        phase: "complete",
        current: total,
        total,
        path: None,
    });
    Ok(pack)
}

pub fn validate_pack_location(
    connection: &Connection,
    pack_id: i64,
    root: &Path,
) -> Result<Vec<(String, i64)>> {
    let indexed_files = {
        let mut statement = connection
            .prepare("SELECT relative_path, size_bytes FROM assets WHERE pack_id = ?1")?;
        let rows = statement
            .query_map(params![pack_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows
    };
    if indexed_files.is_empty() {
        return Err(LootboxError::InvalidPackLocation(
            "there are no indexed files to validate against".into(),
        ));
    }
    let exact_matches = indexed_files
        .iter()
        .filter(|(relative_path, size_bytes)| {
            fs::metadata(root.join(relative_path))
                .is_ok_and(|metadata| metadata.is_file() && metadata.len() == *size_bytes as u64)
        })
        .count();
    let required_matches = if indexed_files.len() <= 4 {
        indexed_files.len()
    } else {
        (indexed_files.len() * 3).div_ceil(5)
    };
    if exact_matches < required_matches {
        return Err(LootboxError::InvalidPackLocation(format!(
            "only {exact_matches} of {} indexed files match",
            indexed_files.len()
        )));
    }
    Ok(indexed_files)
}

