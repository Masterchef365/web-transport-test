pub use serde;
pub use tarpc;
pub use futures;

use bytes::Bytes;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::convert::Infallible;
use std::{marker::PhantomData, sync::Arc, task::Poll};
use tarpc::{transport::channel::UnboundedChannel, Transport};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, ReadHalf, SimplexStream, WriteHalf};
use tokio_util::codec::{Decoder, LengthDelimitedCodec};

use futures::{AsyncRead, Sink, SinkExt, Stream, StreamExt};
use web_transport::{RecvStream, SendStream, Session};

pub struct Framework {
    pub sess: Session,
    pub next_id: usize,
}

/// Don't worry about it
#[cfg(target_arch = "wasm32")]
unsafe impl Send for Framework {}

impl Framework {
    pub fn new(sess: Session) -> Self {
        Self { sess, next_id: 0 }
    }

    fn get_next_id(&mut self) -> usize {
        let next = self.next_id + 1;
        std::mem::replace(&mut self.next_id, next)
    }
}

/// Internal type representing the identity of a connection between client and server
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct BiStream(usize);

/*
/// Uniquely identifies a stream, and carries type information about its contents.
/// This is the type used to transmit information between client and server about the identity of a
/// connected stream/sink combo.
///
/// This is a type you should return from your API, in order to get a bidirectional stream on the other end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TypedBiStream<ClientToServer, ServerToClient> {
    id: BiStream,
    _phantom: PhantomData<(ClientToServer, ServerToClient)>,
}

impl<CTS, STC> TypedBiStream<CTS, STC> {
    pub async fn accept(&self, fr: &mut Framework) -> Box<dyn Stream<CTS> + Sink<STC>> {
        todo!()
    }
}
*/

const BUFFER_SIZE: usize = 4096; // Chosen arbitrarily!
const MAX_READ_BYTES: usize = 4096; // Chosen arbitrarily!

/// This is the type used to provide connectivity to an alternate tarpc connection
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TarpcBiStream(BiStream);

/// Converts a webtransport bidirectional connection into a DuplexStream
/// Warning: spawns tasks underneath
pub fn webtransport_futures_bridge((mut tx, mut rx): (SendStream, RecvStream)) -> DuplexStream {
    let (proxy, ret) = tokio::io::duplex(BUFFER_SIZE);

    let (mut readhalf, mut writehalf) = tokio::io::split(proxy);

    tokio::spawn(async move {
        loop {
            let mut buf = vec![0_u8; BUFFER_SIZE];

            let n_bytes_read = readhalf.read(&mut buf).await?;
            buf.truncate(n_bytes_read);

            tx.write(&buf).await?;
        }

        #[allow(unreachable_code)]
        Ok::<_, FrameworkError>(())
    });

    tokio::spawn(async move {
        loop {
            if let Some(bytes) = rx.read(MAX_READ_BYTES).await? {
                writehalf.write(bytes.as_ref()).await?;
            }
        }

        #[allow(unreachable_code)]
        Ok::<_, FrameworkError>(())
    });

    ret
}

pub fn webtransport_protocol<Rx: DeserializeOwned, Tx: Serialize>(
    socks: (SendStream, RecvStream),
) -> impl Transport<Tx, Rx, Error = FrameworkError> {
    let duplex = webtransport_futures_bridge(socks);

    LengthDelimitedCodec::default()
        .framed(duplex)
        .sink_map_err(FrameworkError::from)
        .with(|obj: Tx| async move { Ok(Bytes::from(encode(&obj)?)) })
        .map(|frame| Ok(decode::<Rx>(&frame?)?))
}

#[derive(thiserror::Error, Debug)]
pub enum FrameworkError {
    #[error("Serialization")]
    Bincode(#[from] bincode::Error),

    #[error("Websocket")]
    WebSocket(#[from] web_transport::Error),

    #[error("Duplex IO")]
    Io(#[from] std::io::Error),
}

/// The encoding function for all data. Mostly for internal use, exposed here for debugging
/// potential
fn encode<T: Serialize>(value: &T) -> bincode::Result<Vec<u8>> {
    bincode::serialize(value)
}

/// The dencoding function for all data. Mostly for internal use, exposed here for debugging
/// potential
fn decode<T: DeserializeOwned>(bytes: &[u8]) -> bincode::Result<T> {
    bincode::deserialize(bytes)
}
