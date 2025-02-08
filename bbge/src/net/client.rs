use std::net::IpAddr;

use renet2::{Bytes, RenetClient};
use renet2_netcode::NetcodeClientTransport;
use renetcode2::ClientAuthentication;

#[cfg(not(target_family = "wasm"))]
pub use std::time::Instant;
#[cfg(target_family = "wasm")]
pub use wasmtimer::std::Instant;

pub struct Client {
    client: RenetClient,
    transport: NetcodeClientTransport,
    last_updated: Instant,
}

impl Client {
    // we must call this function every game tick
    pub fn tick(&mut self) -> anyhow::Result<()> {
        let now = Instant::now();
        let duration = now - self.last_updated;
        self.last_updated = Instant::now();

        self.client.update(duration);
        self.transport.update(duration, &mut self.client)?;

        self.transport.send_packets(&mut self.client)?;

        Ok(())
    }

    pub fn receive_unreliable<D: serde::de::DeserializeOwned>(&mut self) -> Option<anyhow::Result<D>> {
        self.client.receive_message(0).map(|msg| {
            rmp_serde::decode::from_slice(&msg).map_err(|e| e.into())
        })
    }

    pub fn send_unreliable<S: serde::ser::Serialize>(&mut self, data: &S) -> Result<(), rmp_serde::encode::Error>{
        let data = rmp_serde::to_vec(&data)?;
        self.client.send_message(0, Bytes::from(data));

        Ok(())
    }
}

#[cfg(target_family = "wasm")]
pub fn new_client(server_ip: IpAddr) -> anyhow::Result<Client> {
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

    Ok(Client {
        client,
        transport,
        last_updated: Instant::now(),
    })
}

#[cfg(not(target_family = "wasm"))]
pub fn new_client(server_ip: IpAddr) -> anyhow::Result<Client> {
    use std::{net::{SocketAddr, UdpSocket}, time::SystemTime};

    use renet2::{ConnectionConfig, RenetClient};
    use renet2_netcode::NativeSocket;

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server_addr = runtime.block_on(async move {
        reqwest::get(format!("http://{}:4433/native", server_ip.to_string()))
            .await
            .unwrap()
            .json::<SocketAddr>()
            .await
            .unwrap()
    });

    let udp_socket = UdpSocket::bind("127.0.0.1:12345").unwrap();
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

    Ok(Client {
        client,
        transport,
        last_updated: Instant::now(),
    })
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
