//! Test HTTP response parsing

#[test]
fn test_parse_http_response() {
    let response =
        b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 12\r\n\r\nHello World!";

    let response_str = String::from_utf8_lossy(response);

    // Find body start
    let body_start = if let Some(pos) = response_str.find("\r\n\r\n") {
        pos + 4
    } else if let Some(pos) = response_str.find("\n\n") {
        pos + 2
    } else {
        0
    };

    println!("Response length: {}", response.len());
    println!("Body starts at: {}", body_start);
    println!("Body: {:?}", &response[body_start..]);
    println!(
        "Body string: {}",
        String::from_utf8_lossy(&response[body_start..])
    );

    assert_eq!(body_start, 65); // Headers end at byte 65
    assert_eq!(&response[body_start..], b"Hello World!");
    assert_eq!(response[body_start..].len(), 12);
}
