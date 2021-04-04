//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//

use anyhow::Result;
use async_trait::async_trait;

use crate::NodeId;
use crate::Node;

#[async_trait]
pub trait DagBackend<Id, N>
    where N: Node,
          Id: NodeId + Send
{
    async fn get(&self, id: Id) -> Result<Option<N>>;
    async fn put(&mut self, node: N) -> Result<Id>;
}

#[cfg(test)]
mod tests {
    use std::pin::Pin;

    use anyhow::Result;
    use anyhow::anyhow;
    use async_trait::async_trait;
    use tokio_test::block_on;

    use crate::test_impl as test;
    use crate::*;

    #[test]
    fn test_backend_get() {
        let b = test::Backend(vec![Some(test::Node {
            id: test::Id(0),
            parents: vec![],
            data: 42,
        })]);

        let node = tokio_test::block_on(b.get(test::Id(0)));
        assert!(node.is_ok());
        let node = node.unwrap();

        assert!(node.is_some());
        let node = node.unwrap();

        assert_eq!(node.data, 42);
        assert_eq!(node.id, test::Id(0));
        assert!(node.parents.is_empty());
    }

    #[test]
    fn test_backend_put() {
        let mut b = test::Backend(vec![Some(test::Node {
            id: test::Id(0),
            parents: vec![],
            data: 42,
        })]);

        let id = tokio_test::block_on(b.put({
            test::Node {
                id: test::Id(1),
                parents: vec![],
                data: 43,
            }
        }));

        {
            let node = tokio_test::block_on(b.get(test::Id(0)));
            assert!(node.is_ok());
            let node = node.unwrap();

            assert!(node.is_some());
            let node = node.unwrap();

            assert_eq!(node.data, 42);
            assert_eq!(node.id, test::Id(0));
            assert!(node.parents.is_empty());
        }
        {
            let node = tokio_test::block_on(b.get(test::Id(1)));
            assert!(node.is_ok());
            let node = node.unwrap();

            assert!(node.is_some());
            let node = node.unwrap();

            assert_eq!(node.data, 43);
            assert_eq!(node.id, test::Id(1));
            assert!(node.parents.is_empty());
        }
        {
            let node = tokio_test::block_on(b.get(test::Id(2)));
            assert!(node.is_ok());
            let node = node.unwrap();

            assert!(node.is_none());
        }
    }

}
