use super::*;
use std::net::TcpListener;

#[test]
fn base64_matches_the_websocket_rfc_example() {
    assert_eq!(base64(b"the sample nonce"), "dGhlIHNhbXBsZSBub25jZQ==");
    assert_eq!(base64(b"ab"), "YWI=");
    assert_eq!(base64(b"abc"), "YWJj");
}

#[test]
fn client_frames_are_masked_and_sized() {
    let mask = [1, 2, 3, 4];
    let small = frame(0x1, b"hi", mask);
    assert_eq!(&small[..2], &[0x81, 0x82]);
    assert_eq!(&small[2..6], &mask);
    assert_eq!(&small[6..], &[b'h' ^ 1, b'i' ^ 2]);
    let medium = frame(0x1, &[0; 300], mask);
    assert_eq!(&medium[..4], &[0x81, 0x80 | 126, 1, 44]);
}

#[test]
fn pointer_socket_paths_are_reached_on_the_local_port() {
    assert_eq!(
        socket_path("wss://192.168.1.5:3001/resources/abc/netinput.pointer.sock"),
        "/resources/abc/netinput.pointer.sock"
    );
    assert_eq!(socket_path("ws://127.0.0.1:3000/x"), "/x");
}

#[test]
fn page_buttons_map_to_tv_button_names() {
    assert_eq!(button("ok"), Some("ENTER"));
    assert_eq!(button("volume_up"), Some("VOLUMEUP"));
    assert_eq!(button("reboot"), None);
}

/// A tiny server: handshake, then one masked text frame in, one unmasked frame out.
#[test]
fn socket_round_trips_text_with_a_server() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            request.push_str(&line);
            if line == "\r\n" {
                break;
            }
        }
        assert!(request.starts_with("GET /path HTTP/1.1\r\n"));
        assert!(request.contains("Upgrade: websocket"));
        stream
            .write_all(b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n\r\n")
            .unwrap();
        let mut head = [0; 2];
        reader.read_exact(&mut head).unwrap();
        assert_eq!(head[0], 0x81);
        let length = usize::from(head[1] & 0x7f);
        let mut mask = [0; 4];
        reader.read_exact(&mut mask).unwrap();
        let mut payload = vec![0; length];
        reader.read_exact(&mut payload).unwrap();
        let text: Vec<u8> = payload
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index % 4])
            .collect();
        assert_eq!(text, b"hello");
        stream.write_all(&[0x81, 5]).unwrap();
        stream.write_all(b"world").unwrap();
    });
    let mut socket = Socket::connect(&address, "/path").unwrap();
    socket.send_text("hello").unwrap();
    assert_eq!(socket.read_text().unwrap(), "world");
    server.join().unwrap();
}
