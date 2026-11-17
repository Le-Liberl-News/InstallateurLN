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
