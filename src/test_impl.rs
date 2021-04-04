//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//

use anyhow::Result;
use async_trait::async_trait;

#[derive(Copy, Clone, Eq, PartialEq, std::hash::Hash, Debug)]
pub struct Id(pub(crate) usize);

impl crate::NodeId for Id {}

#[derive(Clone, Debug)]
pub struct Node {
    pub(crate) id: Id,
    pub(crate) parents: Vec<Id>,
    pub(crate) data: u8,
}

impl crate::Node for Node {
    type Id = Id;

    fn id(&self) -> &Self::Id {
        &self.id
    }

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
pub struct Backend(pub(crate) Vec<Option<Node>>);

#[async_trait]
impl crate::DagBackend<Id, Node> for Backend {
    async fn get(&self, id: Id) -> Result<Option<Node>> {
        if self.0.len() < id.0 + 1 {
            Ok(None)
        } else {
            Ok(self.0[id.0].clone())
        }
    }

    async fn put(&mut self, node: Node) -> Result<Id> {
        while self.0.len() < node.id.0 + 1 {
            self.0.push(None)
        }

        let idx = node.id.0;
        self.0[idx] = Some(node);
        Ok(Id(idx))
    }
}

