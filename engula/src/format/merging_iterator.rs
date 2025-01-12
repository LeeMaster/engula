use std::{cmp::Reverse, collections::BinaryHeap};

use async_trait::async_trait;

use super::{
    iterator::{Entry, Iterator},
    Timestamp,
};
use crate::error::Result;

pub struct MergingIterator {
    heap: BinaryHeap<Reverse<Box<dyn Iterator>>>,
}

impl MergingIterator {
    pub fn new(children: Vec<Box<dyn Iterator>>) -> MergingIterator {
        let mut heap = BinaryHeap::new();
        for child in children {
            heap.push(Reverse(child));
        }
        MergingIterator { heap }
    }
}

#[async_trait]
impl Iterator for MergingIterator {
    async fn seek_to_first(&mut self) {
        let mut children = Vec::new();
        for mut child in self.heap.drain() {
            child.0.seek_to_first().await;
            children.push(child);
        }
        self.heap = BinaryHeap::from(children);
    }

    async fn seek(&mut self, ts: Timestamp, target: &[u8]) {
        let mut children = Vec::new();
        for mut child in self.heap.drain() {
            child.0.seek(ts, target).await;
            children.push(child);
        }
        self.heap = BinaryHeap::from(children);
    }

    async fn next(&mut self) {
        if let Some(mut top) = self.heap.pop() {
            top.0.next().await;
            self.heap.push(top);
        }
    }

    fn current(&self) -> Result<Option<Entry>> {
        if let Some(top) = self.heap.peek() {
            top.0.current()
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::block::*;
    use super::*;

    #[tokio::test]
    async fn test() {
        let mut builder = BlockBuilder::new();
        let iter1 = {
            let versions: [(Timestamp, &[u8], &[u8]); 3] =
                [(3, &[1], &[3]), (1, &[1], &[1]), (5, &[5], &[5])];
            for v in &versions {
                builder.add(v.0, v.1, v.2);
            }
            let block = builder.finish().to_owned();
            let mut iter = BlockIterator::new(Arc::new(block));
            iter.seek_to_first().await;
            for v in versions {
                assert_eq!(iter.current().unwrap(), Some(v.into()));
                iter.next().await;
            }
            iter
        };
        builder.reset();
        let iter2 = {
            let versions: [(Timestamp, &[u8], &[u8]); 3] =
                [(4, &[2], &[4]), (2, &[2], &[2]), (6, &[6], &[6])];
            for v in &versions {
                builder.add(v.0, v.1, v.2);
            }
            let block = builder.finish().to_owned();
            let mut iter = BlockIterator::new(Arc::new(block));
            iter.seek_to_first().await;
            for v in versions {
                assert_eq!(iter.current().unwrap(), Some(v.into()));
                iter.next().await;
            }
            iter
        };

        let versions: [(Timestamp, &[u8], &[u8]); 6] = [
            (3, &[1], &[3]),
            (1, &[1], &[1]),
            (4, &[2], &[4]),
            (2, &[2], &[2]),
            (5, &[5], &[5]),
            (6, &[6], &[6]),
        ];

        let mut iter = MergingIterator::new(vec![Box::new(iter1), Box::new(iter2)]);
        iter.seek_to_first().await;
        for v in versions {
            assert_eq!(iter.current().unwrap(), Some(v.into()));
            iter.next().await;
        }
        assert_eq!(iter.current().unwrap(), None);
        iter.seek(3, &[2]).await;
        assert_eq!(
            iter.current().unwrap(),
            Some(Entry(2, [2].as_ref(), [2].as_ref()))
        );
        iter.next().await;
        assert_eq!(
            iter.current().unwrap(),
            Some(Entry(5, [5].as_ref(), [5].as_ref()))
        );
        iter.next().await;
        assert_eq!(
            iter.current().unwrap(),
            Some(Entry(6, [6].as_ref(), [6].as_ref()))
        );
        iter.next().await;
        assert_eq!(iter.current().unwrap(), None);
    }
}
