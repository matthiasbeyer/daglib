//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//

use std::sync::Arc;
use std::sync::RwLock;

use async_trait::async_trait;

#[derive(Debug)]
pub struct TestError;

impl<Id> From<crate::Error<Id>> for TestError
where
    Id: crate::NodeId,
{
    fn from(_ce: crate::Error<Id>) -> Self {
        TestError
    }
}

#[derive(Copy, Clone, Eq, PartialEq, std::hash::Hash, Debug)]
pub struct Id(pub(crate) usize);

impl crate::NodeId for Id {}

#[derive(Clone, Debug)]
pub struct Node {
    pub(crate) parents: Vec<Id>,
    // data the node holds, used to create the ID in tests as "hashing" for unique id
    pub(crate) data: usize,
}

impl crate::Node for Node {
    type Id = Id;

    fn parent_ids(&self) -> Vec<Self::Id> {
        self.parents.clone()
    }
}

/// The backend for the tests
///
/// This is `Clone` because we can test branching only with a clonable backend.
/// A real backend would not implement the storage itself, but rather a way to retrieve the data
/// from some storage mechansim (think IPFS), and thus `Clone`ing a backend is nothing esotheric.
#[derive(Clone, Debug)]
pub struct Backend(pub(crate) Arc<RwLock<Vec<Option<Node>>>>);

impl Backend {
    pub fn new(v: Vec<Option<Node>>) -> Self {
        Backend(Arc::new(RwLock::new(v)))
    }
}

#[async_trait]
impl crate::DagBackend<Id, Node> for Backend {
    type Error = TestError;

    async fn get(&self, id: Id) -> Result<Option<(Id, Node)>, Self::Error> {
        if self.0.read().unwrap().len() < id.0 + 1 {
            Ok(None)
        } else {
            Ok(self.0.read().unwrap()[id.0].clone().map(|node| (id, node)))
        }
    }

    async fn put(&mut self, node: Node) -> Result<Id, Self::Error> {
        while self.0.read().unwrap().len() < node.data + 1 {
            self.0.write().unwrap().push(None)
        }

        let idx = node.data;
        self.0.write().unwrap()[idx] = Some(node);
        Ok(Id(idx))
    }
}
