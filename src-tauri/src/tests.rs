use std::{
    collections::HashSet,
    fs,
    path::Path,
    sync::atomic::AtomicBool,
};
use rusqlite::{params, Connection};

use crate::{
    commands::*, db::*, error::*, godot::*, ingest::*, models::*,
};


    #[test]
    fn naturally_sorts_numeric_runs_in_asset_names() {
        let connection = Connection::open_in_memory().unwrap();
        register_collations(&connection).unwrap();
        connection
            .execute("CREATE TABLE names (name TEXT NOT NULL)", [])
            .unwrap();
        for name in [
            "ambience_d19_loop",
            "ambience_d2_loop",
            "ambience_d10_loop",
            "ambience_d1_loop",
        ] {
            connection
                .execute("INSERT INTO names(name) VALUES (?1)", [name])
                .unwrap();
        }
        let mut statement = connection
            .prepare("SELECT name FROM names ORDER BY name COLLATE LOOTBOX_NATURAL ASC")
            .unwrap();
        let names = statement
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            names,
            [
                "ambience_d1_loop",
                "ambience_d2_loop",
                "ambience_d10_loop",
                "ambience_d19_loop"
            ]
        );
    }

    #[test]
    fn naturally_sorts_packs_collections_and_projects() {
        let connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();

        for (name, path) in [
            ("pack vol 56", "/packs/p56"),
            ("pack vol 9", "/packs/p9"),
            ("pack vol 2", "/packs/p2"),
            ("pack vol 100", "/packs/p100"),
        ] {
            connection
                .execute(
                    "INSERT INTO packs(name, root_path) VALUES (?1, ?2)",
                    params![name, path],
                )
                .unwrap();
        }

        let mut statement = connection
            .prepare("SELECT name FROM packs ORDER BY name COLLATE LOOTBOX_NATURAL ASC")
            .unwrap();
        let pack_names = statement
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(
            pack_names,
            [
                "pack vol 2",
                "pack vol 9",
                "pack vol 56",
                "pack vol 100",
            ]
        );

        for name in ["Hero 100", "Hero 9", "Hero 20", "Hero 2"] {
            connection
                .execute(
                    "INSERT INTO collections(name) VALUES (?1)",
                    params![name],
                )
                .unwrap();
        }

        let mut coll_statement = connection
            .prepare("SELECT name FROM collections ORDER BY name COLLATE LOOTBOX_NATURAL ASC")
            .unwrap();
        let coll_names = coll_statement
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(
            coll_names,
            [
                "Hero 2",
                "Hero 9",
                "Hero 20",
                "Hero 100",
            ]
        );
    }


    #[test]
    fn classifies_common_game_asset_formats() {
        assert_eq!(classify_extension("png"), "image");
        assert_eq!(
            classify_asset_type(Path::new("Textures/stone.png"), "png"),
            "texture"
        );
        assert_eq!(
            classify_asset_type(Path::new("References/stone.png"), "png"),
            "image"
        );
        assert_eq!(classify_extension("wav"), "audio");
        assert_eq!(classify_extension("glb"), "model");
        assert_eq!(classify_extension("wgsl"), "shader");
        assert_eq!(classify_extension("prefab"), "other");
        assert_eq!(
            classify_asset_type(Path::new("512/Color Maps/brick.png"), "png"),
            "texture"
        );
        assert_eq!(
            classify_asset_type(Path::new("brick_normal.png"), "png"),
            "texture"
        );
        assert_eq!(
            texture_group_key(Path::new("256/Color Maps/brick.png")),
            texture_group_key(Path::new("512/Normal Maps/brick_normal.png"))
        );
    }

    #[test]
    fn groups_model_formats_across_export_directories() {
        let glb = model_variant_group(
            Path::new("Models/GLB (recommended)/Props/crate.glb"),
            "model",
            "glb",
        );
        let fbx = model_variant_group(
            Path::new("Models/other-formats/FBX/Props/crate.fbx"),
            "model",
            "fbx",
        );
        let mtl = model_variant_group(
            Path::new("Models/other-formats/OBJ/Props/crate.mtl"),
            "material",
            "mtl",
        );
        assert_eq!(glb, fbx);
        assert_eq!(glb, mtl);
        assert_ne!(
            glb,
            model_variant_group(
                Path::new("Models/GLB (recommended)/Buildings/crate.glb"),
                "model",
                "glb"
            )
        );
    }

    #[test]
    fn creates_safe_prefix_searches() {
        assert_eq!(
            fts_query("wooden sword"),
            Some("\"wooden\"* AND \"sword\"*".to_string())
        );
        assert_eq!(fts_query("!!!"), None);
    }

    #[test]
    fn initializes_an_empty_database() {
        let connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM assets", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
        let reverse_collection_index: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = 'collection_assets_asset_idx')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(reverse_collection_index);
    }

    #[test]
    fn groups_texture_maps_and_resolution_variants() {
        let temporary = tempfile::tempdir().unwrap();
        let pack = temporary.path().join("Surface Pack");
        for resolution in ["256", "512"] {
            fs::create_dir_all(pack.join(resolution).join("Color Maps")).unwrap();
            fs::create_dir_all(pack.join(resolution).join("Normal Maps")).unwrap();
            let size = if resolution == "512" { 16 } else { 8 };
            image::RgbaImage::from_pixel(size, size, image::Rgba([120, 90, 70, 255]))
                .save(pack.join(resolution).join("Color Maps/wall.png"))
                .unwrap();
            image::RgbaImage::from_pixel(size, size, image::Rgba([128, 128, 255, 255]))
                .save(pack.join(resolution).join("Normal Maps/wall_normal.png"))
                .unwrap();
        }

        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        let imported =
            import_pack_from_path(&mut connection, &pack, None, None, &mut |_| {}).unwrap();
        assert_eq!(imported.asset_count, 1);

        let page = query_assets_from_connection(
            AssetQuery {
                query: None,
                asset_type: Some("texture".into()),
                pack_id: Some(imported.id),
                collection_id: None,
                limit: None,
                offset: None,
                excluded: None,
                sort: None,
                ..AssetQuery::default()
            },
            &connection,
        )
        .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].relative_path, "512/Color Maps/wall.png");
        assert_eq!(page.items[0].variants.len(), 4);
        assert_eq!(page.items[0].resources.len(), 3);
    }

    #[test]
    fn classifies_generic_texture_conventions_from_correlated_evidence() {
        let temporary = tempfile::tempdir().unwrap();
        let pack = temporary.path().join("Mixed Asset Pack");
        fs::create_dir_all(pack.join("Materials/Brick")).unwrap();
        fs::create_dir_all(pack.join("Materials/Stone")).unwrap();
        fs::create_dir_all(pack.join("Unreal")).unwrap();
        fs::create_dir_all(pack.join("References")).unwrap();
        for path in [
            "Materials/Brick/Albedo.png",
            "Materials/Brick/NormalGL.png",
            "Materials/Stone/stone.png",
            "Materials/Stone/stone_normal.png",
            "Unreal/T_Metal_D.png",
            "Unreal/T_Metal_N.png",
            "Unreal/T_Metal_ORM.png",
            "References/hero_color.png",
        ] {
            image::RgbaImage::from_pixel(8, 8, image::Rgba([120, 90, 70, 255]))
                .save(pack.join(path))
                .unwrap();
        }

        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        let imported =
            import_pack_from_path(&mut connection, &pack, None, None, &mut |_| {}).unwrap();
        assert_eq!(imported.asset_count, 4);

        let texture_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM assets WHERE pack_id = ?1 AND usage = 'texture' AND is_primary = 1",
                params![imported.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(texture_count, 3);

        let inferred_role: String = connection
            .query_row(
                "SELECT map_role FROM assets WHERE pack_id = ?1 AND relative_path = 'Materials/Stone/stone.png'",
                params![imported.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(inferred_role, "color");

        let (asset_type, usage, confidence, basis): (String, Option<String>, i64, String) =
            connection
                .query_row(
                    "SELECT asset_type, usage, classification_confidence, classification_basis FROM assets WHERE pack_id = ?1 AND relative_path = 'References/hero_color.png'",
                    params![imported.id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .unwrap();
        assert_eq!(asset_type, "image");
        assert_eq!(usage, None);
        assert_eq!(confidence, 55);
        assert!(basis.contains("map-role-filename"));

        let roles: HashSet<String> = {
            let mut statement = connection
                .prepare("SELECT map_role FROM assets WHERE pack_id = ?1 AND usage = 'texture'")
                .unwrap();
            statement
                .query_map(params![imported.id], |row| row.get(0))
                .unwrap()
                .collect::<std::result::Result<_, _>>()
                .unwrap()
        };
        assert!(roles.contains("color"));
        assert!(roles.contains("normal_gl"));
        assert!(roles.contains("occlusion_roughness_metalness"));
    }

    #[test]
    fn migrates_existing_rows_to_the_versioned_classifier() {
        let temporary = tempfile::tempdir().unwrap();
        let pack = temporary.path().join("Migrated Pack");
        fs::create_dir_all(pack.join("Brick")).unwrap();
        for path in ["Brick/Albedo.png", "Brick/NormalDX.png"] {
            image::RgbaImage::from_pixel(8, 8, image::Rgba([128, 128, 128, 255]))
                .save(pack.join(path))
                .unwrap();
        }

        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        import_pack_from_path(&mut connection, &pack, None, None, &mut |_| {}).unwrap();
        connection.execute("DELETE FROM app_metadata", []).unwrap();
        connection
            .execute(
                "UPDATE assets SET file_type = 'other', usage = NULL, map_role = NULL, group_key = NULL, variant_group = NULL, asset_type = 'image'",
                [],
            )
            .unwrap();

        migrate_classification(&mut connection).unwrap();
        let classified: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM assets WHERE file_type = 'image' AND usage = 'texture' AND group_key IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(classified, 2);
        let version: String = connection
            .query_row(
                "SELECT value FROM app_metadata WHERE key = 'classification_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, "2");
        migrate_classification(&mut connection).unwrap();
    }

    #[test]
    fn imports_and_rescans_a_real_folder() {
        let temporary = tempfile::tempdir().unwrap();
        let pack = temporary.path().join("Starter Pack");
        fs::create_dir_all(pack.join("models")).unwrap();
        fs::create_dir_all(pack.join("models/other-formats/FBX")).unwrap();
        fs::create_dir_all(pack.join("models/other-formats/OBJ")).unwrap();
        fs::create_dir_all(pack.join("models/other-formats/DAE")).unwrap();
        fs::create_dir_all(pack.join("textures")).unwrap();
        fs::create_dir_all(pack.join(".ignored")).unwrap();
        fs::write(pack.join("models").join("wooden_sword.glb"), b"glb").unwrap();
        fs::write(
            pack.join("models/other-formats/FBX/wooden_sword.fbx"),
            b"fbx",
        )
        .unwrap();
        fs::write(
            pack.join("models/other-formats/OBJ/wooden_sword.obj"),
            b"obj",
        )
        .unwrap();
        fs::write(
            pack.join("models/other-formats/OBJ/wooden_sword.mtl"),
            b"newmtl wooden_sword\nmap_Kd C:/sword_diffuse.png\n",
        )
        .unwrap();
        fs::write(pack.join("impact.wav"), b"wave").unwrap();
        image::RgbaImage::from_pixel(8, 4, image::Rgba([150, 175, 140, 255]))
            .save(pack.join("grass.png"))
            .unwrap();
        image::RgbaImage::from_pixel(4, 4, image::Rgba([90, 110, 80, 255]))
            .save(pack.join("textures/sword_diffuse.png"))
            .unwrap();
        image::RgbaImage::from_pixel(4, 4, image::Rgba([90, 110, 80, 255]))
            .save(pack.join("models/other-formats/DAE/sword_diffuse.png"))
            .unwrap();
        fs::write(pack.join(".ignored").join("secret.png"), b"ignored").unwrap();

        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        let thumbnails = temporary.path().join("thumbnails");
        let mut progress = Vec::new();
        let imported = import_pack_from_path(
            &mut connection,
            &pack,
            Some(&thumbnails),
            None,
            &mut |event| progress.push(event),
        )
        .unwrap();
        assert_eq!(imported.name, "Starter Pack");
        assert_eq!(imported.asset_count, 3);
        assert!(imported.available);
        assert_eq!(progress.first().unwrap().phase, "scanning");
        assert_eq!(progress.last().unwrap().phase, "complete");
        assert_eq!(progress.last().unwrap().current, 8);
        assert!(validate_pack_location(&connection, imported.id, &pack).is_ok());
        let wrong_location = temporary.path().join("Wrong Pack");
        fs::create_dir(&wrong_location).unwrap();
        assert!(validate_pack_location(&connection, imported.id, &wrong_location).is_err());

        let model_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM assets WHERE asset_type = 'model'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let search_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM assets_fts WHERE assets_fts MATCH '\"wooden\"*'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(model_count, 3);
        let primary_model_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM assets WHERE asset_type = 'model' AND is_primary = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let grouped_variant_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM assets WHERE variant_group IS NOT NULL AND is_primary = 0",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(primary_model_count, 1);
        assert_eq!(grouped_variant_count, 5);
        let texture_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM assets WHERE asset_type = 'texture' AND is_primary = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(texture_count, 0);
        let dependency_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM asset_dependencies", [], |row| {
                row.get(0)
            })
            .unwrap();
        let dependency_search_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM assets_fts WHERE assets_fts MATCH '\"sword_diffuse\"*'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(dependency_count, 1);
        assert_eq!(dependency_search_count, 1);
        assert_eq!(search_count, 1);

        let selection_query = AssetQuery {
            query: Some("wooden".into()),
            asset_type: Some("model".into()),
            sort: Some("name".into()),
            ..AssetQuery::default()
        };
        let lightweight_ids = query_asset_selections_from_connection(&selection_query, &connection)
            .unwrap()
            .into_iter()
            .map(|selection| selection.id)
            .collect::<Vec<_>>();
        let full_ids = query_assets_from_connection(selection_query, &connection)
            .unwrap()
            .items
            .into_iter()
            .map(|asset| asset.id)
            .collect::<Vec<_>>();
        assert_eq!(lightweight_ids, full_ids);

        let first_page = query_assets_from_connection(
            AssetQuery {
                query: None,
                asset_type: None,
                pack_id: None,
                collection_id: None,
                limit: Some(1),
                offset: Some(0),
                excluded: None,
                sort: None,
                ..AssetQuery::default()
            },
            &connection,
        )
        .unwrap();
        let second_page = query_assets_from_connection(
            AssetQuery {
                query: None,
                asset_type: None,
                pack_id: None,
                collection_id: None,
                limit: Some(1),
                offset: Some(1),
                excluded: None,
                sort: None,
                ..AssetQuery::default()
            },
            &connection,
        )
        .unwrap();
        assert_eq!(first_page.total, 3);
        assert_eq!(first_page.items.len(), 1);
        assert!(first_page.has_more);
        assert_ne!(first_page.items[0].id, second_page.items[0].id);

        let ascending = query_assets_from_connection(
            AssetQuery {
                sort: Some("name".into()),
                sort_direction: Some("asc".into()),
                limit: Some(10),
                ..AssetQuery::default()
            },
            &connection,
        )
        .unwrap()
        .items
        .into_iter()
        .map(|asset| asset.name)
        .collect::<Vec<_>>();
        let descending = query_assets_from_connection(
            AssetQuery {
                sort: Some("name".into()),
                sort_direction: Some("desc".into()),
                limit: Some(10),
                ..AssetQuery::default()
            },
            &connection,
        )
        .unwrap()
        .items
        .into_iter()
        .map(|asset| asset.name)
        .collect::<Vec<_>>();
        assert_eq!(
            ascending.iter().rev().cloned().collect::<Vec<_>>(),
            descending
        );

        let (width, height, thumbnail): (i64, i64, String) = connection
            .query_row(
                "SELECT width, height, thumbnail_path FROM assets WHERE name = 'grass'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!((width, height), (8, 4));
        assert!(Path::new(&thumbnail).is_file());

        let model_id: i64 = connection
            .query_row(
                "SELECT id FROM assets WHERE asset_type = 'model' AND is_primary = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        set_assets_excluded_from_connection(&[model_id], true, &mut connection).unwrap();
        let after_exclusion = query_assets_from_connection(
            AssetQuery {
                query: None,
                asset_type: None,
                pack_id: None,
                collection_id: None,
                limit: Some(10),
                offset: Some(0),
                excluded: None,
                sort: None,
                ..AssetQuery::default()
            },
            &connection,
        )
        .unwrap();
        assert_eq!(after_exclusion.total, 2);
        let removed_page = query_assets_from_connection(
            AssetQuery {
                query: None,
                asset_type: None,
                pack_id: Some(imported.id),
                collection_id: None,
                limit: Some(10),
                offset: Some(0),
                excluded: Some(true),
                sort: Some("largest".into()),
                ..AssetQuery::default()
            },
            &connection,
        )
        .unwrap();
        assert_eq!(removed_page.total, 1);
        assert_eq!(
            get_pack(&connection, imported.id)
                .unwrap()
                .removed_asset_count,
            1
        );
        set_assets_excluded_from_connection(&[model_id], false, &mut connection).unwrap();
        assert_eq!(
            query_assets_from_connection(
                AssetQuery {
                    query: None,
                    asset_type: None,
                    pack_id: None,
                    collection_id: None,
                    limit: Some(10),
                    offset: Some(0),
                    excluded: None,
                    sort: Some("type".into()),
                    ..AssetQuery::default()
                },
                &connection,
            )
            .unwrap()
            .total,
            3
        );
        set_assets_excluded_from_connection(&[model_id], true, &mut connection).unwrap();
        connection
            .execute(
                "UPDATE packs SET name = 'My Starter Pack' WHERE id = ?1",
                params![imported.id],
            )
            .unwrap();
        let rescanned =
            import_pack_from_path(&mut connection, &pack, Some(&thumbnails), None, &mut |_| {})
                .unwrap();
        assert_eq!(rescanned.asset_count, 2);
        assert_eq!(rescanned.name, "My Starter Pack");

        fs::remove_file(pack.join("impact.wav")).unwrap();
        let rescanned =
            import_pack_from_path(&mut connection, &pack, Some(&thumbnails), None, &mut |_| {})
                .unwrap();
        assert_eq!(rescanned.asset_count, 1);
    }

    #[test]
    fn rescans_preserve_identity_metadata_and_missing_records() {
        let temporary = tempfile::tempdir().unwrap();
        let pack = temporary.path().join("Rename Pack");
        fs::create_dir_all(&pack).unwrap();
        image::RgbaImage::from_pixel(8, 8, image::Rgba([10, 20, 30, 255]))
            .save(pack.join("old-name.png"))
            .unwrap();
        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        let imported =
            import_pack_from_path(&mut connection, &pack, None, None, &mut |_| {}).unwrap();
        let original_id: i64 = connection
            .query_row(
                "SELECT id FROM assets WHERE pack_id = ?1",
                params![imported.id],
                |row| row.get(0),
            )
            .unwrap();
        connection
            .execute("INSERT INTO tags(name) VALUES ('favorite')", [])
            .unwrap();
        connection.execute(
            "INSERT INTO asset_tags(asset_id, tag_id) SELECT ?1, id FROM tags WHERE name = 'favorite'",
            params![original_id],
        ).unwrap();

        fs::rename(pack.join("old-name.png"), pack.join("new-name.png")).unwrap();
        import_pack_from_path(&mut connection, &pack, None, None, &mut |_| {}).unwrap();
        let (renamed_id, relative_path): (i64, String) = connection
            .query_row(
                "SELECT id, relative_path FROM assets WHERE pack_id = ?1",
                params![imported.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(renamed_id, original_id);
        assert_eq!(relative_path, "new-name.png");
        let tag_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM asset_tags WHERE asset_id = ?1",
                params![original_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tag_count, 1);

        fs::remove_file(pack.join("new-name.png")).unwrap();
        let rescanned =
            import_pack_from_path(&mut connection, &pack, None, None, &mut |_| {}).unwrap();
        assert_eq!(rescanned.asset_count, 0);
        assert_eq!(rescanned.missing_asset_count, 1);
        let (missing, retained_tags): (bool, i64) = connection.query_row(
            "SELECT missing, (SELECT COUNT(*) FROM asset_tags WHERE asset_id = assets.id) FROM assets WHERE id = ?1",
            params![original_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert!(missing);
        assert_eq!(retained_tags, 1);
    }

    #[test]
    fn manual_classification_overrides_survive_recomputation() {
        let temporary = tempfile::tempdir().unwrap();
        let pack = temporary.path().join("Override Pack");
        fs::create_dir_all(&pack).unwrap();
        image::RgbaImage::from_pixel(8, 8, image::Rgba([10, 20, 30, 255]))
            .save(pack.join("reference.png"))
            .unwrap();
        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        let imported =
            import_pack_from_path(&mut connection, &pack, None, None, &mut |_| {}).unwrap();
        let asset_id: i64 = connection
            .query_row("SELECT id FROM assets", [], |row| row.get(0))
            .unwrap();
        connection.execute(
            "INSERT INTO classification_overrides(asset_id, asset_type, map_role, group_key) VALUES (?1, 'texture', 'color', 'manual:test')",
            params![asset_id],
        ).unwrap();
        recompute_texture_groups(&connection, Some(imported.id)).unwrap();
        apply_classification_overrides(&connection, Some(imported.id)).unwrap();
        let values: (String, Option<String>, String, String) = connection.query_row(
            "SELECT asset_type, map_role, group_key, classification_basis FROM assets WHERE id = ?1",
            params![asset_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ).unwrap();
        assert_eq!(
            values,
            (
                "texture".into(),
                Some("color".into()),
                "manual:test".into(),
                "manual-override".into()
            )
        );
        connection
            .execute(
                "UPDATE classification_overrides SET map_role = '__none' WHERE asset_id = ?1",
                params![asset_id],
            )
            .unwrap();
        recompute_texture_groups(&connection, Some(imported.id)).unwrap();
        apply_classification_overrides(&connection, Some(imported.id)).unwrap();
        let cleared_role: Option<String> = connection
            .query_row(
                "SELECT map_role FROM assets WHERE id = ?1",
                params![asset_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(cleared_role, None);
    }

    #[test]
    fn import_cancellation_stops_before_writing() {
        let temporary = tempfile::tempdir().unwrap();
        let pack = temporary.path().join("Cancelled Pack");
        fs::create_dir_all(&pack).unwrap();
        fs::write(pack.join("asset.txt"), b"asset").unwrap();
        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        let cancelled = AtomicBool::new(true);
        let result =
            import_pack_from_path(&mut connection, &pack, None, Some(&cancelled), &mut |_| {});
        assert!(matches!(result, Err(LootboxError::ImportCancelled)));
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM packs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn detects_duplicates_by_file_content_across_packs() {
        let temporary = tempfile::tempdir().unwrap();
        let first_pack = temporary.path().join("First Pack");
        let second_pack = temporary.path().join("Second Pack");
        fs::create_dir_all(&first_pack).unwrap();
        fs::create_dir_all(&second_pack).unwrap();
        fs::write(first_pack.join("impact.wav"), b"identical audio bytes").unwrap();
        fs::write(second_pack.join("renamed.wav"), b"identical audio bytes").unwrap();
        fs::write(second_pack.join("different.wav"), b"different audio bytes").unwrap();

        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        import_pack_from_path(&mut connection, &first_pack, None, None, &mut |_| {}).unwrap();
        import_pack_from_path(&mut connection, &second_pack, None, None, &mut |_| {}).unwrap();

        let page = query_assets_from_connection(
            AssetQuery {
                duplicates_only: Some(true),
                ..AssetQuery::default()
            },
            &connection,
        )
        .unwrap();
        assert_eq!(page.total, 2);
        assert!(page.items.iter().all(|asset| asset.content_hash.is_some()));
        assert!(page.items.iter().all(|asset| asset.duplicate_count == 2));
        assert!(page
            .items
            .iter()
            .all(|asset| asset.duplicate_locations.len() == 1));
    }

    #[test]
    fn exports_grouped_texture_maps_to_a_godot_project_idempotently() {
        let temporary = tempfile::tempdir().unwrap();
        let pack = temporary.path().join("Surface Pack");
        let project = temporary.path().join("Godot Game");
        fs::create_dir_all(pack.join("Materials/Brick")).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("project.godot"),
            b"[application]\nconfig/name=\"Export Test\"\n",
        )
        .unwrap();
        image::RgbaImage::from_pixel(8, 8, image::Rgba([100, 70, 50, 255]))
            .save(pack.join("Materials/Brick/brick_color.png"))
            .unwrap();
        image::RgbaImage::from_pixel(8, 8, image::Rgba([128, 128, 255, 255]))
            .save(pack.join("Materials/Brick/brick_normal.png"))
            .unwrap();

        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        import_pack_from_path(&mut connection, &pack, None, None, &mut |_| {}).unwrap();
        connection
            .execute(
                "INSERT INTO projects(name, root_path) VALUES ('Export Test', ?1)",
                params![path_string(&project)],
            )
            .unwrap();
        let project_id = connection.last_insert_rowid();
        let asset_id: i64 = connection
            .query_row(
                "SELECT id FROM assets WHERE is_primary = 1 AND asset_type = 'texture'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let exported_root = project.join("assets/lootbox/surface-pack-1/Materials/Brick");
        fs::create_dir_all(&exported_root).unwrap();
        fs::write(exported_root.join("brick_color.png"), b"project-owned file").unwrap();

        let preview =
            preview_assets_to_godot_from_connection(&connection, project_id, &[asset_id], None)
                .unwrap();
        assert_eq!(preview.selected, 1);
        assert_eq!(preview.related, 1);
        assert_eq!(preview.grouped, 1);
        assert_eq!(preview.dependencies, 0);
        assert_eq!(preview.total_files, 2);
        assert_eq!(preview.conflicts, 1);
        assert_eq!(preview.conflict_files.len(), 1);
        assert_eq!(preview.destination, "res://assets/lootbox");

        let first =
            export_assets_to_godot_from_connection(&mut connection, project_id, &[asset_id], None)
                .unwrap();
        assert_eq!(first.copied, 2);
        assert_eq!(first.unchanged, 0);
        assert_eq!(
            fs::read(exported_root.join("brick_color.png")).unwrap(),
            b"project-owned file"
        );
        assert!(exported_root
            .join(format!("brick_color-lootbox-{asset_id}.png"))
            .is_file());
        assert!(exported_root.join("brick_normal.png").is_file());
        assert!(project
            .join("assets/lootbox/lootbox-manifest.json")
            .is_file());

        let second =
            export_assets_to_godot_from_connection(&mut connection, project_id, &[asset_id], None)
                .unwrap();
        assert_eq!(second.copied, 0);
        assert_eq!(second.unchanged, 2);

        let status = project_status_from_connection(&connection, project_id).unwrap();
        assert_eq!(status.tracked_files, 2);
        assert_eq!(status.up_to_date_files, 2);
        assert_eq!(status.runs.len(), 2);
        assert_eq!(status.runs[0].unchanged_count, 2);

        let unused = query_assets_from_connection(
            AssetQuery {
                unused_by_projects: Some(true),
                ..AssetQuery::default()
            },
            &connection,
        )
        .unwrap();
        assert_eq!(unused.total, 0);

        fs::write(
            pack.join("Materials/Brick/brick_normal.png"),
            b"changed source",
        )
        .unwrap();
        let changed_status = project_status_from_connection(&connection, project_id).unwrap();
        assert_eq!(changed_status.source_changed_files, 1);
        assert_eq!(changed_status.up_to_date_files, 1);

        let normal_id: i64 = connection
            .query_row(
                "SELECT id FROM assets WHERE relative_path LIKE '%brick_normal.png'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let edited_project_path: String = connection
            .query_row(
                "SELECT exported_path FROM project_exports WHERE project_id = ?1 AND asset_id = ?2",
                params![project_id, normal_id],
                |row| row.get(0),
            )
            .unwrap();
        fs::write(&edited_project_path, b"project edit").unwrap();
        let protected_preview =
            preview_assets_to_godot_from_connection(&connection, project_id, &[asset_id], None)
                .unwrap();
        assert!(protected_preview.conflicts >= 2);
        export_assets_to_godot_from_connection(&mut connection, project_id, &[asset_id], None)
            .unwrap();
        assert_eq!(fs::read(&edited_project_path).unwrap(), b"project edit");
        let replacement_path: String = connection
            .query_row(
                "SELECT exported_path FROM project_exports WHERE project_id = ?1 AND asset_id = ?2",
                params![project_id, normal_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(replacement_path, edited_project_path);

        let moved_project = temporary.path().join("Moved Godot Game");
        fs::rename(&project, &moved_project).unwrap();
        let relocated =
            relocate_godot_project_from_connection(&mut connection, project_id, &moved_project)
                .unwrap();
        assert_eq!(relocated.root_path, path_string(&moved_project));
        let relocated_paths = connection
            .prepare("SELECT exported_path FROM project_exports WHERE project_id = ?1")
            .unwrap()
            .query_map(params![project_id], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert!(relocated_paths
            .iter()
            .all(|path| Path::new(path).starts_with(&moved_project)));
        let relocated_status = project_status_from_connection(&connection, project_id).unwrap();
        assert_eq!(relocated_status.tracked_files, 2);
        assert_eq!(relocated_status.up_to_date_files, 2);
        let removal =
            plan_assets_from_godot_project_removal(&connection, project_id, &[asset_id]).unwrap();
        assert_eq!(removal.preview.remove_files.len(), 2);
        assert!(removal.preview.missing_files.is_empty());
    }

    #[test]
    fn filters_model_export_formats_but_keeps_required_companions() {
        let temporary = tempfile::tempdir().unwrap();
        let pack = temporary.path().join("Model Pack");
        let project = temporary.path().join("Godot Game");
        fs::create_dir_all(pack.join("Models/GLB")).unwrap();
        fs::create_dir_all(pack.join("Models/other-formats/FBX")).unwrap();
        fs::create_dir_all(pack.join("Models/other-formats/OBJ")).unwrap();
        fs::create_dir_all(pack.join("Textures")).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("project.godot"),
            b"[application]\nconfig/name=\"Format Test\"\n",
        )
        .unwrap();
        fs::write(pack.join("Models/GLB/crate.glb"), b"glb model").unwrap();
        fs::write(
            pack.join("Models/other-formats/FBX/crate.fbx"),
            b"fbx model",
        )
        .unwrap();
        fs::write(
            pack.join("Models/other-formats/OBJ/crate.obj"),
            b"mtllib crate.mtl\n",
        )
        .unwrap();
        fs::write(
            pack.join("Models/other-formats/OBJ/crate.mtl"),
            b"map_Kd crate_diffuse.png\n",
        )
        .unwrap();
        image::RgbaImage::from_pixel(8, 8, image::Rgba([100, 70, 50, 255]))
            .save(pack.join("Textures/crate_diffuse.png"))
            .unwrap();

        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        import_pack_from_path(&mut connection, &pack, None, None, &mut |_| {}).unwrap();
        connection
            .execute(
                "INSERT INTO projects(name, root_path) VALUES ('Format Test', ?1)",
                params![path_string(&project)],
            )
            .unwrap();
        let project_id = connection.last_insert_rowid();
        let asset_id: i64 = connection
            .query_row(
                "SELECT id FROM assets WHERE is_primary = 1 AND asset_type = 'model'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        let all_formats =
            preview_assets_to_godot_from_connection(&connection, project_id, &[asset_id], None)
                .unwrap();
        assert_eq!(
            all_formats
                .model_formats
                .iter()
                .map(|format| format.extension.as_str())
                .collect::<Vec<_>>(),
            vec!["glb", "fbx", "obj"]
        );
        assert_eq!(
            all_formats.selected_model_formats,
            vec!["fbx", "glb", "obj"]
        );

        let glb = vec!["glb".to_string()];
        let glb_only = preview_assets_to_godot_from_connection(
            &connection,
            project_id,
            &[asset_id],
            Some(&glb),
        )
        .unwrap();
        assert_eq!(glb_only.selected_model_formats, vec!["glb"]);
        assert!(glb_only
            .files
            .iter()
            .any(|file| file.ends_with("crate.glb")));
        assert!(!glb_only
            .files
            .iter()
            .any(|file| file.ends_with("crate.fbx")));
        assert!(!glb_only
            .files
            .iter()
            .any(|file| file.ends_with("crate.obj")));
        assert!(!glb_only
            .files
            .iter()
            .any(|file| file.ends_with("crate.mtl")));
        assert!(glb_only
            .files
            .iter()
            .any(|file| file.ends_with("crate_diffuse.png")));

        let obj = vec!["obj".to_string()];
        let obj_only = preview_assets_to_godot_from_connection(
            &connection,
            project_id,
            &[asset_id],
            Some(&obj),
        )
        .unwrap();
        assert!(obj_only
            .files
            .iter()
            .any(|file| file.ends_with("crate.obj")));
        assert!(obj_only
            .files
            .iter()
            .any(|file| file.ends_with("crate.mtl")));
        assert!(obj_only
            .files
            .iter()
            .any(|file| file.ends_with("crate_diffuse.png")));

        let exported = export_assets_to_godot_from_connection(
            &mut connection,
            project_id,
            &[asset_id],
            Some(&glb),
        )
        .unwrap();
        assert_eq!(exported.copied, glb_only.total_files);
    }

    #[test]
    fn removes_only_unchanged_project_exports_and_keeps_shared_or_modified_files() {
        let temporary = tempfile::tempdir().unwrap();
        let pack = temporary.path().join("Model Pack");
        let project = temporary.path().join("Godot Game");
        for directory in [
            "Models/GLB",
            "Models/other-formats/FBX",
            "Models/other-formats/OBJ",
            "Textures",
        ] {
            fs::create_dir_all(pack.join(directory)).unwrap();
        }
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join("project.godot"),
            b"[application]\nconfig/name=\"Removal Test\"\n",
        )
        .unwrap();
        fs::write(pack.join("Models/GLB/crate.glb"), b"crate glb").unwrap();
        fs::write(
            pack.join("Models/other-formats/FBX/crate.fbx"),
            b"crate fbx",
        )
        .unwrap();
        fs::write(
            pack.join("Models/other-formats/OBJ/crate.mtl"),
            b"map_Kd shared.png\n",
        )
        .unwrap();
        fs::write(pack.join("Models/GLB/barrel.glb"), b"barrel glb").unwrap();
        fs::write(
            pack.join("Models/other-formats/OBJ/barrel.mtl"),
            b"map_Kd shared.png\n",
        )
        .unwrap();
        image::RgbaImage::from_pixel(8, 8, image::Rgba([90, 60, 30, 255]))
            .save(pack.join("Textures/shared.png"))
            .unwrap();

        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        import_pack_from_path(&mut connection, &pack, None, None, &mut |_| {}).unwrap();
        connection
            .execute(
                "INSERT INTO projects(name, root_path) VALUES ('Removal Test', ?1)",
                params![path_string(&project)],
            )
            .unwrap();
        let project_id = connection.last_insert_rowid();
        let crate_id: i64 = connection
            .query_row(
                "SELECT id FROM assets WHERE name = 'crate' AND extension = 'glb'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let barrel_id: i64 = connection
            .query_row(
                "SELECT id FROM assets WHERE name = 'barrel' AND extension = 'glb'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        export_assets_to_godot_from_connection(&mut connection, project_id, &[crate_id], None)
            .unwrap();
        export_assets_to_godot_from_connection(&mut connection, project_id, &[barrel_id], None)
            .unwrap();

        let crate_glb: String = connection
            .query_row(
                "SELECT exported_path FROM project_exports WHERE project_id = ?1 AND asset_id = ?2",
                params![project_id, crate_id],
                |row| row.get(0),
            )
            .unwrap();
        fs::write(&crate_glb, b"project edited crate glb").unwrap();

        let preview = plan_assets_from_godot_project_removal(&connection, project_id, &[crate_id])
            .unwrap()
            .preview;
        assert_eq!(preview.selected, 1);
        assert!(preview
            .remove_files
            .iter()
            .any(|path| path.ends_with("crate.fbx")));
        assert!(preview
            .modified_files
            .iter()
            .any(|path| path.ends_with("crate.glb")));
        assert!(preview
            .shared_files
            .iter()
            .any(|path| path.ends_with("shared.png")));

        let result = remove_assets_from_godot_project_from_connection(
            &mut connection,
            project_id,
            &[crate_id],
        )
        .unwrap();
        assert_eq!(result.deleted, 1);
        assert_eq!(result.kept_modified, 1);
        assert_eq!(result.kept_shared, 1);
        assert!(Path::new(&crate_glb).is_file());
        assert_eq!(
            project_summary(&connection, project_id)
                .unwrap()
                .asset_count,
            1
        );

        let manifest =
            fs::read_to_string(project.join("assets/lootbox/lootbox-manifest.json")).unwrap();
        assert!(!manifest.contains("crate.glb"));
        assert!(!manifest.contains("crate.fbx"));
        assert!(manifest.contains("barrel.glb"));
        assert!(manifest.contains("shared.png"));
    }

    #[test]
    fn packaged_release_preview_policy_is_safe() {
        let config = include_str!("../tauri.conf.json");
        let parsed: serde_json::Value = serde_json::from_str(config).unwrap();
        let csp = parsed["app"]["security"]["csp"].as_str().unwrap();
        assert!(csp.contains("connect-src") && csp.contains("blob:") && csp.contains("data:"));
        assert!(csp.contains("asset:") && csp.contains("http://asset.localhost"));
        let model_preview = include_str!("../../src/components/ModelPreview.tsx");
        assert!(model_preview.contains("GLTFLoader"));
        assert!(model_preview.contains("outputColorSpace = THREE.SRGBColorSpace"));
    }

    #[test]
    fn extracts_model_poly_and_vertex_counts() {
        let gltf_json = r#"{
            "accessors": [
                { "count": 24 },
                { "count": 36 }
            ],
            "meshes": [
                {
                    "primitives": [
                        {
                            "attributes": { "POSITION": 0 },
                            "indices": 1,
                            "mode": 4
                        }
                    ]
                }
            ]
        }"#;
        let counts = parse_gltf_json(gltf_json.as_bytes()).unwrap();
        assert_eq!(counts, (12, 24));

        let dir = tempfile::tempdir().unwrap();
        let obj_path = dir.path().join("cube.obj");
        fs::write(
            &obj_path,
            "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3\nf 1 3 4\n",
        )
        .unwrap();
        let (triangles, vertices) = model_poly_count(&obj_path, "obj");
        assert_eq!(triangles, Some(2));
        assert_eq!(vertices, Some(4));
    }
