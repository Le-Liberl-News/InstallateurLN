use crate::patch;
use std::io::{Read, Write};

#[test]
fn interrupted_transfer_resumes_and_handles_servers_ignoring_ranges() {
    for ignore_range in [false, true] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let payload = vec![b'x'; 262144];
        let expected = payload.clone();
        let server = std::thread::spawn(move || {
            for attempt in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut headers=Vec::new();
                while !headers.ends_with(b"\r\n\r\n") { let mut byte=[0];stream.read_exact(&mut byte).unwrap();headers.push(byte[0]); }
                let headers=String::from_utf8(headers).unwrap().to_lowercase();
                if attempt==0 {
                    write!(stream,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: \"fixture\"\r\nConnection: close\r\n\r\n",payload.len()).unwrap();
                    stream.write_all(&payload[..131072]).unwrap();
                } else {
                    let range=headers.lines().find_map(|line|line.strip_prefix("range: bytes=")).expect("retry requests the missing bytes");
                    let start:usize=range.trim_end_matches('-').parse().unwrap();
                    assert!(start>0 && start<payload.len());
                    if ignore_range {
                        write!(stream,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",payload.len()).unwrap();
                        stream.write_all(&payload).unwrap();
                    } else {
                        assert!(headers.contains("if-range: \"fixture\""));
                        write!(stream,"HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n",payload.len()-start,start,payload.len()-1,payload.len()).unwrap();
                        stream.write_all(&payload[start..]).unwrap();
                    }
                }
            }
        });
        let folder=std::env::temp_dir().join(format!("ln-download-{}-{}",std::process::id(),address.port()));
        std::fs::create_dir_all(&folder).unwrap();
        let target=folder.join("archive.zip");
        let result=patch::download(&format!("http://{address}/archive"),&target,expected.len() as u64,"fixture",&Default::default());
        server.join().unwrap();
        result.unwrap();
        assert_eq!(std::fs::read(&target).unwrap(),expected);
        std::fs::remove_dir_all(folder).unwrap();
    }
}

#[test]
fn unrecognised_game_files_are_named_before_anything_is_downloaded() {
    let folder = std::env::temp_dir().join(format!("ln-unknown-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let kit = folder.join("kit");
    std::fs::create_dir_all(&kit).unwrap();
    std::fs::write(folder.join("ed6_win.exe"), b"another store's exe").unwrap();
    std::fs::write(folder.join("ED6_DT01.dat"), b"steam").unwrap();
    let steam = patch::sha256(&folder.join("ED6_DT01.dat"));
    let manifest = format!(
        r#"{{"version":"test","files":[{{"file":"ed6_win.exe","original_sha256":"{0}0","patched_sha256":"{0}1","delta":"missing-a.vcdiff"}},{{"file":"ED6_DT01.dat","original_sha256":"{steam}","patched_sha256":"{0}2","delta":"missing-b.vcdiff"}}]}}"#,
        "f".repeat(63)
    );
    std::fs::write(kit.join("patch.json"), manifest).unwrap();
    let game = crate::games::GAMES.iter().find(|game| game.project == "sky-fc").unwrap();
    let error = patch::install(game, &folder, &patch::Source::Folder(kit), &Default::default()).unwrap_err();
    assert!(error.starts_with("Version du jeu non prise en charge"), "{error}");
    assert!(error.contains("• ed6_win.exe") && !error.contains("ED6_DT01.dat"), "{error}");
    assert_eq!(std::fs::read(folder.join("ed6_win.exe")).unwrap(), b"another store's exe");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_version_of_the_players_game_is_picked_among_the_variants() {
    let folder = std::env::temp_dir().join(format!("ln-variant-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let kit = folder.join("kit");
    std::fs::create_dir_all(kit.join("deltas")).unwrap();
    let xdelta = patch::xdelta3().unwrap();
    let make = |name: &str, english: &[u8], french: &[u8]| {
        let (source, target) = (folder.join(format!("{name}.en")), folder.join(format!("{name}.fr")));
        std::fs::write(&source, english).unwrap();
        std::fs::write(&target, french).unwrap();
        let delta = kit.join("deltas").join(format!("{name}.xdelta"));
        let status = std::process::Command::new(&xdelta).arg("-e").arg("-f").arg("-s").arg(&source).arg(&target).arg(&delta).status().unwrap();
        assert!(status.success());
        (patch::sha256(&source), patch::sha256(&target))
    };
    let (steam, steam_fr) = make("steam", b"steam exe, september 2022", b"exe steam, septembre 2022");
    let (old, old_fr) = make("old", b"older exe, july 2021", b"ancien exe, juillet 2021");
    std::fs::write(folder.join("ed6_win.exe"), b"older exe, july 2021").unwrap();
    let manifest = format!(
        r#"{{"version":"test","files":[{{"file":"ed6_win.exe","original_sha256":"{steam}","patched_sha256":"{steam_fr}","delta":"deltas/steam.xdelta","variants":[{{"version":"2021-07","original_sha256":"{old}","patched_sha256":"{old_fr}","delta":"deltas/old.xdelta"}}]}}]}}"#
    );
    std::fs::write(kit.join("patch.json"), manifest).unwrap();
    let game = crate::games::GAMES.iter().find(|game| game.project == "sky-fc").unwrap();
    let source = patch::Source::Folder(kit);
    patch::install(game, &folder, &source, &Default::default()).unwrap();
    assert_eq!(std::fs::read(folder.join("ed6_win.exe")).unwrap(), b"ancien exe, juillet 2021");
    assert_eq!(std::fs::read(folder.join("dat_en").join("ed6_win.exe")).unwrap(), b"older exe, july 2021");
    // Installed again: already this version's French file.
    patch::install(game, &folder, &source, &Default::default()).unwrap();
    assert_eq!(std::fs::read(folder.join("ed6_win.exe")).unwrap(), b"ancien exe, juillet 2021");
    let _ = std::fs::remove_dir_all(&folder);
}
