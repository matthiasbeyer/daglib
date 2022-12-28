//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//

use async_trait::async_trait;

use crate::Node;
use crate::NodeId;

/// An interface to a DAG backend storage
///
/// A DAG backend storage is nothing more than a thing that can store (`DagBackend::put`) and load
/// (`DagBackend::get`) nodes.
#[async_trait]
pub trait DagBackend<Id, N>
where
    N: Node,
    Id: NodeId + Send,
{
    type Error;

    /// Get a `Node` from the backend that is identified by `id`
    ///
    /// # Returns
    ///
    /// * Should return Err(_) if the operation failed.
    /// * Should return Ok(None) if there is no node that is identified by `id`
    /// * Otherwise return the Id along with the node identified by it
    async fn get(&self, id: Id) -> Result<Option<(Id, N)>, Self::Error>;

    /// Store a `node` in the backend, returning its `Id`
    ///
    /// This function should store the `node` in the backend and return the `id` the node has.
    async fn put(&mut self, node: N) -> Result<Id, Self::Error>;
}

#[cfg(test)]
mod tests {
    use crate::test_impl as test;
    use crate::*;

    #[test]
    fn test_backend_get() {
        let b = test::Backend::new(vec![Some(test::Node {
            parents: vec![],
            data: 0,
        })]);

        let node = tokio_test::block_on(b.get(test::Id(0)));
        assert!(node.is_ok());
        let node = node.unwrap();

        assert!(node.is_some());
        let node = node.unwrap();

        assert_eq!(node.0, test::Id(0));
        assert!(node.1.parents.is_empty());
    }

    #[test]
    fn test_backend_put() {
        let mut b = test::Backend::new(vec![Some(test::Node {
            parents: vec![],
            data: 0,
        })]);

        let _ = tokio_test::block_on(b.put({
            test::Node {
                parents: vec![],
                data: 1,
            }
        }));

        {
            let node = tokio_test::block_on(b.get(test::Id(0)));
            assert!(node.is_ok());
            let node = node.unwrap();

            assert!(node.is_some());
            let node = node.unwrap();

            assert_eq!(node.0, test::Id(0));
            assert!(node.1.parents.is_empty());
        }
        {
            let node = tokio_test::block_on(b.get(test::Id(1)));
            assert!(node.is_ok());
            let node = node.unwrap();

            assert!(node.is_some());
            let node = node.unwrap();

            assert_eq!(node.0, test::Id(1));
            assert!(node.1.parents.is_empty());
        }
        {
            let node = tokio_test::block_on(b.get(test::Id(2)));
            assert!(node.is_ok());
            let node = node.unwrap();

            assert!(node.is_none());
        }
    }
}
