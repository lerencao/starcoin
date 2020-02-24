// Copyright (c) The Starcoin Core Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{GetCounterMessage, PeerMessage,NetworkMessage};
use actix::prelude::*;
use anyhow::Result;
use bus::{Broadcast, BusActor};
use config::{NetworkConfig, NodeConfig};
use futures_03::{
    compat::Stream01CompatExt,
    TryFutureExt,
};
use libp2p::{
    identity,
    ping::{Ping, PingConfig, PingEvent},
    PeerId, Swarm,
};
use std::sync::Arc;
use traits::TxPoolAsyncService;
use txpool::{AddTransaction, TxPoolActor};
use types::{system_events::SystemEvents, transaction::SignedUserTransaction};
use crate::net::{build_network_service,NetworkService};
use crypto::ed25519::{Ed25519PrivateKey, Ed25519PublicKey};
use crypto::{test_utils::KeyPair, Uniform};
use scs::SCSCodec;

use futures::{
    stream::Stream,
    sync::{
        mpsc,
        oneshot,
    }
};

pub struct NetworkActor<P>
where
    P: TxPoolAsyncService,
    P: 'static,
{
    network_service:NetworkService,
    tx:mpsc::UnboundedSender<NetworkMessage>,
    tx_command:oneshot::Sender<()>,
    bus: Addr<BusActor>,
    txpool: P,
    //just for test, remove later.
    counter: u64,
}

impl<P> NetworkActor<P>
where
    P: TxPoolAsyncService,
{
    pub fn launch(
        node_config: Arc<NodeConfig>,
        bus: Addr<BusActor>,
        txpool: P,
        key_pair: Arc<KeyPair<Ed25519PrivateKey, Ed25519PublicKey>>,
    ) -> Result<Addr<NetworkActor<P>>> {
        //TODO read from config

        let (service,tx,rx,tx_command)=build_network_service(&node_config.network,key_pair);
        info!("network started at {} with seed {},network address is {}",&node_config.network.listen,&node_config.network.seeds.iter().fold(String::new(), |acc, arg| acc + arg.as_str()),service.identify());
        Ok(NetworkActor::create(
            move |ctx: &mut Context<NetworkActor<P>>| {
                ctx.add_stream(rx.fuse().compat());
                NetworkActor {
                    network_service:service,
                    tx,
                    tx_command,
                    bus,
                    txpool,
                    counter: 0,
                }
            },
        ))
    }
}

impl<P> Actor for NetworkActor<P>
where
    P: TxPoolAsyncService,
{
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!(
            "Network actor started ",
        );
    }
}

impl<P> StreamHandler<Result<NetworkMessage,()>> for NetworkActor<P>
    where
        P: TxPoolAsyncService,
{
    fn handle(&mut self, item: Result<NetworkMessage,()>, ctx: &mut Self::Context) {
        match item {
            Ok(network_msg)=>{
                info!("receive network_message {:?}", network_msg);
                let message = PeerMessage::decode(&network_msg.data);
                match message {
                    Ok(msg)=>{
                        ctx.notify(msg);
                    },
                    Err(e)=>{
                        warn!("get error {:?}",e);
                    }
                }
            },
            Err(e)=>{
                warn!("get error {:?}",e);
            }
        }
    }
}

/// handler system events.
impl<P> Handler<SystemEvents> for NetworkActor<P>
where
    P: TxPoolAsyncService,
{
    type Result = ();

    fn handle(&mut self, msg: SystemEvents, _ctx: &mut Self::Context) -> Self::Result {
        match msg {
            SystemEvents::NewUserTransaction(txn) => {
                debug!("new user transaction {:?}",txn);
                let peer_msg = PeerMessage::UserTransaction(txn);
                let bytes = peer_msg.encode().unwrap();
                self.network_service.broadcast_message(bytes);
            }
            SystemEvents::NewHeadBlock(block) => {
                //TODO broadcast block to peers.
            }
            _ => {}
        }
    }
}

/// Handler for receive broadcast from other peer.
impl<P> Handler<PeerMessage> for NetworkActor<P>
where
    P: TxPoolAsyncService,
{
    type Result = ResponseActFuture<Self, Result<()>>;

    fn handle(&mut self, msg: PeerMessage, _ctx: &mut Self::Context) -> Self::Result {
        match msg {
            PeerMessage::UserTransaction(txn) => {
                let f = self.txpool.clone().add(txn).and_then(|new_txn| async move {
                    info!("add tx success, is new tx: {}", new_txn);
                    Ok(())
                });
                let f = actix::fut::wrap_future(f);
                Box::new(f)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use txpool::TxPoolActor;
    use traits::mock::MockTxPoolService;
    use types::account_address::AccountAddress;

    use log::{Record, Level, Metadata};
    use log::{SetLoggerError, LevelFilter};

    struct SimpleLogger;

    impl log::Log for SimpleLogger {
        fn enabled(&self, metadata: &Metadata) -> bool {
            metadata.level() <= Level::Info
        }

        fn log(&self, record: &Record) {
            if self.enabled(record.metadata()) {
                println!("{} - {}", record.level(), record.args());
            }
        }

        fn flush(&self) {}
    }

    static LOGGER: SimpleLogger = SimpleLogger;

    fn init_log() -> Result<(), SetLoggerError> {
        log::set_logger(&LOGGER)
            .map(|()| log::set_max_level(LevelFilter::Info))
    }
    // #[actix_rt::test]
    // async fn test_network() {
    //     let node_config = NodeConfig::default();
    //     let bus = BusActor::launch();
    //     let txpool = TxPoolActor::launch(&node_config, bus.clone()).unwrap();
    //     let network = NetworkActor::launch(&node_config, bus, txpool.clone()).unwrap();
    //     network
    //         .send(PeerMessage::UserTransaction(SignedUserTransaction::mock()))
    //         .await
    //         .unwrap();
    //
    //     let txns = txpool.get_pending_txns().await.unwrap();
    //     assert_eq!(1, txns.len());
    // }

    #[actix_rt::test]
    async fn test_network_with_mock() {
        init_log();

        let mut node_config1 = NodeConfig::default();
        node_config1.network.listen = format!("/ip4/127.0.0.1/tcp/{}", get_available_port());
        let node_config1 = Arc::new(node_config1);

        let (txpool1, network1,addr1) = build_network(node_config1.clone());

        let mut node_config2 = NodeConfig::default();
        let addr1_hex = hex::encode(addr1);
        let seed = format!("{}/p2p/{}", &node_config1.network.listen, addr1_hex);
        node_config2.network.listen = format!("/ip4/127.0.0.1/tcp/{}", get_available_port());
        node_config2.network.seeds = vec![seed];
        let node_config2 = Arc::new(node_config2);

        let (txpool2, network2,addr2) = build_network(node_config2.clone());

        use std::thread;
        use std::time::Duration;

        thread::sleep(Duration::from_millis(1000));

        network1
            .send(SystemEvents::NewUserTransaction(SignedUserTransaction::mock()))
            .await
            .unwrap();

        network2
            .send(SystemEvents::NewUserTransaction(SignedUserTransaction::mock()))
            .await
            .unwrap();

        let txns = txpool1.get_pending_txns().await.unwrap();
        //assert_eq!(1, txns.len());

        let txns = txpool2.get_pending_txns().await.unwrap();
        //assert_eq!(1, txns.len());

    }

    fn build_network(node_config:Arc<NodeConfig>) -> (MockTxPoolService, Addr<NetworkActor<MockTxPoolService>>,AccountAddress) {
        let bus = BusActor::launch();
        let key_pair = gen_keypair();
        let addr = AccountAddress::from_public_key(&key_pair.public_key);
        let txpool = traits::mock::MockTxPoolService::new();
        let network = NetworkActor::launch(node_config.clone(), bus, txpool.clone(), key_pair).unwrap();
        (txpool, network,addr)
    }

    fn gen_keypair()->Arc<KeyPair<Ed25519PrivateKey, Ed25519PublicKey>>{
        use rand::prelude::*;

        let mut seed_rng = rand::rngs::OsRng::new().expect("can't access OsRng");
        let seed_buf: [u8; 32] = seed_rng.gen();
        let mut rng0: StdRng = SeedableRng::from_seed(seed_buf);
        let account_keypair: Arc<KeyPair<Ed25519PrivateKey, Ed25519PublicKey>> =
            Arc::new(KeyPair::generate_for_testing(&mut rng0));
        account_keypair
    }

    fn get_available_port() -> u16 {
        const MAX_PORT_RETRIES: u32 = 1000;

        for _ in 0..MAX_PORT_RETRIES {
            if let Ok(port) = get_ephemeral_port() {
                return port;
            }
        }

        panic!("Error: could not find an available port");
    }

    fn get_ephemeral_port() -> ::std::io::Result<u16> {
        use std::{
            net::{TcpListener, TcpStream},
        };

        // Request a random available port from the OS
        let listener = TcpListener::bind(("localhost", 0))?;
        let addr = listener.local_addr()?;

        // Create and accept a connection (which we'll promptly drop) in order to force the port
        // into the TIME_WAIT state, ensuring that the port will be reserved from some limited
        // amount of time (roughly 60s on some Linux systems)
        let _sender = TcpStream::connect(addr)?;
        let _incoming = listener.accept()?;

        Ok(addr.port())
    }

}
