use renet2::RenetClient;
use renet2_netcode::NetcodeClientTransport;
use renetcode2::ClientAuthentication;

#[cfg(not(target_family = "wasm"))]
use std::time::SystemTime;
#[cfg(target_family = "wasm")]
use wasmtimer::std::SystemTime;

#[cfg(target_family = "wasm")]
pub fn new_renet_client() -> (RenetClient, NetcodeClientTransport) {
    use renet2::ConnectionConfig;
    use renet2_netcode::{
        webtransport_is_available_with_cert_hashes, ClientSocket, CongestionControl,
        NetcodeClientTransport, ServerCertHash, WebServerDestination, WebTransportClient,
        WebTransportClientConfig,
    };

    log::info!("aboout to send req");
    let resp = make_http_request("http://127.0.0.1:4433/wasm");
    let (server_addr, wt_server_cert_hash, _ws_server_url) =
        serde_json::from_str::<(WebServerDestination, ServerCertHash, url::Url)>(&resp).unwrap();
    log::info!("recved");

    let socket_config = WebTransportClientConfig {
        server_dest: server_addr.clone().into(),
        congestion_control: CongestionControl::default(),
        server_cert_hashes: Vec::from([wt_server_cert_hash]),
    };
    let socket = WebTransportClient::new(socket_config);

    let current_time = wasmtimer::std::SystemTime::now()
        .duration_since(wasmtimer::std::SystemTime::UNIX_EPOCH)
        .unwrap();
    let client_id = current_time.as_millis() as u64;

    let client_auth = ClientAuthentication::Unsecure {
        socket_id: 1,
        server_addr: server_addr.clone().into(),
        client_id,
        user_data: None,
        protocol_id: 0,
    };

    let client = RenetClient::new(ConnectionConfig::test(), socket.is_reliable());
    let transport = NetcodeClientTransport::new(current_time, client_auth, socket).unwrap();

    (client, transport)
}

#[cfg(target_family = "wasm")]
fn make_http_request(url: &str) -> String {
    use wasm_bindgen::JsValue;
    use web_sys::XmlHttpRequest;
    let xhr = XmlHttpRequest::new().unwrap();

    // false as the third parameter makes it synchronous
    xhr.open_with_async("GET", url, false).unwrap();
    xhr.send().unwrap();

    if xhr.status().unwrap() == 200 {
        xhr.response_text().unwrap().unwrap()
    } else {
        panic!(
            "{}",
            &format!("Request failed with status: {}", xhr.status().unwrap())
        );
    }
}

#[cfg(not(target_family = "wasm"))]
pub fn new_renet_client() -> (RenetClient, NetcodeClientTransport) {
    use std::net::{SocketAddr, UdpSocket};

    use renet2::{ConnectionConfig, RenetClient};
    use renet2_netcode::NativeSocket;

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server_addr = runtime.block_on(async move {
        reqwest::get("http://127.0.0.1:4433/native")
            .await
            .unwrap()
            .json::<SocketAddr>()
            .await
            .unwrap()
    });

    let mut udp_socket = UdpSocket::bind("127.0.0.1:12345").unwrap();
    udp_socket
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    let client_socket = NativeSocket::new(udp_socket).unwrap();
    let current_time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    let client_id = current_time.as_millis() as u64;
    let authentication = ClientAuthentication::Unsecure {
        socket_id: 0, // Socket id 0 for native sockets in this example.
        server_addr,
        client_id,
        user_data: None,
        protocol_id: 0,
    };

    let client = RenetClient::new(ConnectionConfig::test(), false);
    let transport =
        NetcodeClientTransport::new(current_time, authentication, client_socket).unwrap();

    (client, transport)
}
