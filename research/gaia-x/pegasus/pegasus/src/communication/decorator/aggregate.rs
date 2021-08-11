use crate::channel_id::ChannelInfo;
use crate::communication::decorator::evented::EventEmitPush;
use crate::communication::decorator::ScopeStreamPush;
use crate::communication::IOResult;
use crate::data::MicroBatch;
use crate::data_plane::GeneralPush;
use crate::event::emitter::EventEmitter;
use crate::graph::Port;
use crate::progress::EndSignal;
use crate::{Data, Tag};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct AggregateBatchPush<D: Data> {
    pub ch_info: ChannelInfo,
    data_push: EventEmitPush<D>,
    event_push: Vec<EventEmitPush<D>>,
    has_cycles: Arc<AtomicBool>,
}

impl<D: Data> AggregateBatchPush<D> {
    pub fn new(
        info: ChannelInfo, source_worker: u32, target: u32, has_cycles: Arc<AtomicBool>,
        push: Vec<GeneralPush<MicroBatch<D>>>, emitter: EventEmitter,
    ) -> Self {
        let mut event_push = Vec::with_capacity(push.len());
        for (t, p) in push.into_iter().enumerate() {
            let target_worker = t as u32;
            let has_cycles = has_cycles.clone();
            let p = EventEmitPush::new(info, source_worker, target_worker, has_cycles, p, emitter.clone());
            event_push.push(p);
        }

        let data_push = event_push.swap_remove(target as usize);
        AggregateBatchPush { ch_info: info, data_push, event_push, has_cycles }
    }
}

impl<T: Data> ScopeStreamPush<MicroBatch<T>> for AggregateBatchPush<T> {
    fn port(&self) -> Port {
        self.ch_info.source_port
    }

    fn push(&mut self, tag: &Tag, msg: MicroBatch<T>) -> IOResult<()> {
        self.data_push.push(tag, msg)
    }

    fn push_last(&mut self, msg: MicroBatch<T>, end: EndSignal) -> IOResult<()> {
        if msg.tag.is_root() {
            for p in self.event_push.iter_mut() {
                p.notify_end(end.clone())?;
            }
        }
        self.data_push.push_last(msg, end)
    }

    fn notify_end(&mut self, end: EndSignal) -> IOResult<()> {
        if end.tag.is_root()
            || end.tag.len() < self.ch_info.scope_level
            || self.has_cycles.load(Ordering::SeqCst)
        {
            for p in self.event_push.iter_mut() {
                p.notify_end(end.clone())?;
            }
        }
        self.data_push.notify_end(end)
    }

    fn flush(&mut self) -> IOResult<()> {
        self.data_push.flush()
    }

    fn close(&mut self) -> IOResult<()> {
        for p in self.event_push.iter_mut() {
            p.close()?;
        }
        self.data_push.close()
    }
}

///////////////////////////////////////////////////
