use super::*;

// #[test]
// fn generating_and_load() {
//     let mut song = Song::new();
//     song.header.is_loop = true;
//     for i in 0..25 {
//         song.notes.insert((i, 0, 0, i + 33).try_into().unwrap());
//     }
//     song.save_nbs("evil_cat_world_ruling_scheme/test_song.nbs")
//         .unwrap();

//     let song = Song::open_nbs("evil_cat_world_ruling_scheme/test_song.nbs").unwrap();
//     for note in song.notes {
//         println!("tick: {}, key: {}", note.tick, note.key)
//     }
// }

#[test]
fn test() {
    let mut song = Song::open_nbs("evil_cat_world_ruling_scheme/source.nbs").unwrap();
    let mut notes = Vec::from(song.notes);
    // let song_length: Index = notes
    //     .iter()
    //     .map(|n| n.tick + 1)
    //     .max()
    //     .unwrap_or_default()
    //     .next_power_of_two();

    let rules: [(Index, Index); _] = [
        // (6, 16),
        // (12, 8),
        // (24, 4),
        // (48, 2),
        // (4, 32),
        (3, 12),
        (9, 16),
        (18, 8),
        (36, 4),
        (72, 2),
        (36, 2),
        (1, 1),
    ];
    let mut slices: Vec<Vec<Note>> = Default::default();
    for (stride, freq) in rules {
        let (matches, orphan) = notes.cyclic_matches(stride, freq, Note::tone);

        slices.push(matches.clone());
        notes = orphan;
        // notes.append(&mut matches);
        // notes.sort();
    }

    let mut base_layer: Index = Default::default();
    let mut result: Vec<Note> = Default::default();

    for notes in slices {
        let mut current_layer: Index = Default::default();
        let mut max_layer: Index = Default::default();

        let mut notes = notes.into_iter().peekable();
        while let Some(mut note) = notes.next() {
            note.layer = base_layer + current_layer;

            max_layer = max_layer.max(current_layer + 2);
            match notes.peek().map(|n| n.tick) == Some(note.tick) {
                true => current_layer += 1,
                false => current_layer = 0,
            }

            result.push(note);
        }
        base_layer += max_layer;
    }

    song.notes = result.into();
    song.header.is_loop = true;
    song.save_nbs("evil_cat_world_ruling_scheme/out.nbs")
        .unwrap();
}

// #[test]
// fn test_save_and_load_consistency() {
//     // 创建一个测试歌曲
//     let mut original_song = Song::new();

//     // 设置头部信息
//     original_song.header.song_name = "Test Song".to_string();
//     original_song.header.song_author = "Test Author".to_string();
//     original_song.header.tempo = 12.5;

//     // 添加一些音符
//     original_song.notes.push(Note {
//         tick: 0,
//         layer: 0,
//         instrument: 0,
//         key: 30,
//         velocity: Volume::new(100).unwrap(),
//         panning: Panning::new(0).unwrap(),
//         pitch: 0,
//     });

//     original_song.notes.push(Note {
//         tick: 10,
//         layer: 1,
//         instrument: 1,
//         key: 34,
//         velocity: Volume::new(80).unwrap(),
//         panning: Panning::new(20).unwrap(),
//         pitch: 100,
//     });

//     original_song.notes.push(Note {
//         tick: 5,
//         layer: 0,
//         instrument: 2,
//         key: 37,
//         velocity: Volume::new(75).unwrap(),
//         panning: Panning::new(-30).unwrap(),
//         pitch: 50,
//     });

//     // 添加层
//     original_song.layers.push(Layer {
//         id: 0,
//         name: "Layer 1".to_string(),
//         lock: false,
//         volume: Volume::new(100).unwrap(),
//         panning: Panning::new(0).unwrap(),
//     });

//     original_song.layers.push(Layer {
//         id: 1,
//         name: "Layer 2".to_string(),
//         lock: true,
//         volume: Volume::new(80).unwrap(),
//         panning: Panning::new(10).unwrap(),
//     });

//     let file_path = "evil_cat_world_ruling_scheme/test_song.nbs";

//     // 保存歌曲到文件
//     let save_result = original_song.save_nbs(&file_path);
//     assert!(
//         save_result.is_ok(),
//         "Failed to save song: {:?}",
//         save_result.err()
//     );

//     // 从文件加载歌曲
//     let load_result = Song::open_nbs(&file_path);
//     assert!(
//         load_result.is_ok(),
//         "Failed to load song: {:?}",
//         load_result.err()
//     );

//     let loaded_song = load_result.unwrap();

//     // 比较排序后的歌曲
//     assert_eq!(
//         original_song, loaded_song,
//         "Original and loaded songs should be identical after sorting"
//     );
// }
