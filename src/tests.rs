use super::*;

#[test]
fn generating_and_load() {
    let mut song = Song::new();
    song.header.is_loop = true;
    for i in 0..25u8 {
        song.notes.push({
            Note::new(i.into(), 0, 0, i + 33)
        })
    }
    song.save_nbs("evil_cat_world_ruling_scheme/test_song.nbs").unwrap();

    let song = Song::open_nbs("evil_cat_world_ruling_scheme/test_song.nbs").unwrap();
    for note in song.notes {
        println!("tick: {}, key: {}", note.tick, note.key)
    }
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
