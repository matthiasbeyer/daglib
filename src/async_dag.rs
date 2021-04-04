//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//

use std::pin::Pin;

use anyhow::anyhow;
use anyhow::Result;
use futures::stream::StreamExt;
use futures::task::Poll;

use crate::DagBackend;
use crate::Node;
use crate::NodeId;

/// An async DAG, generic over Node, Node identifier and Backend implementation
pub struct AsyncDag<Id, N, Backend>
where
    Id: NodeId + Send,
    N: Node<Id = Id>,
    Backend: DagBackend<Id, N>,
{
    head: Id,
    backend: Backend,
    _node: std::marker::PhantomData<N>,
}

impl<Id, N, Backend> AsyncDag<Id, N, Backend>
where
    Id: NodeId + Send,
    N: Node<Id = Id>,
    Backend: DagBackend<Id, N>,
{
    /// Create a new DAG with a backend and a HEAD node
    ///
    /// The head node is `DagBackend::put` into the backend before the AsyncDag object is created.
    pub async fn new(mut backend: Backend, head: N) -> Result<Self> {
        backend.put(head).await.map(|id| AsyncDag {
            head: id,
            backend,
            _node: std::marker::PhantomData,
        })
    }

    /// Load a AsyncDag object using `head` as HEAD node.
    ///
    /// # Warning
    ///
    /// This fails if backend.get(head) fails.
    pub async fn load(backend: Backend, head: Id) -> Result<Self> {
        backend
            .get(head)
            .await?
            .map(|(id, _)| AsyncDag {
                head: id,
                backend,
                _node: std::marker::PhantomData,
            })
            .ok_or_else(|| anyhow!("Node not found"))
    }

    pub fn head(&self) -> &Id {
        &self.head
    }

    pub fn backend(&self) -> &Backend {
        &self.backend
    }

    pub fn backend_mut(&mut self) -> &mut Backend {
        &mut self.backend
    }

    /// Check whether an `id` is in the DAG.
    pub async fn has_id(&self, id: &Id) -> Result<bool> {
        self.stream()
            .map(|r| -> Result<bool> { r.map(|(ref node_id, _)| node_id == id) })
            .collect::<Vec<Result<bool>>>()
            .await
            .into_iter()
            .fold(Ok(false), |acc, e| match (acc, e) {
                (Err(e), _) => Err(e),
                (Ok(_), Err(e)) => Err(e),
                (Ok(a), Ok(b)) => Ok(a || b),
            })
    }

    /// Iterate over the DAG
    ///
    /// This function returns a Stream over all nodes in the DAG.
    ///
    /// # Warning
    ///
    /// The order of the nodes is not (yet) guaranteed.
    pub fn stream(&self) -> Stream<Id, N, Backend> {
        Stream {
            dag: self,
            backlog: {
                let mut v = Vec::with_capacity(2);
                v.push(self.backend.get(self.head.clone()));
                v
            },
        }
    }

    /// Update the HEAD pointer of the DAG
    ///
    /// # Warning
    ///
    /// fails if the passed node does not point to the current HEAD in its parents.
    pub async fn update_head(&mut self, node: N) -> Result<Id> {
        if node.parent_ids().iter().any(|id| id == &self.head) {
            self.update_head_unchecked(node).await
        } else {
            Err(anyhow!("Node does not have HEAD as parent"))
        }
    }

    /// Update the HEAD pointer of the DAG, unchecked
    ///
    /// # Warning
    ///
    /// Does not check whether the passed `node` does point to the current (then old) HEAD in its
    /// parents. Be careful to not lose nodes from the DAG with this.
    pub async fn update_head_unchecked(&mut self, node: N) -> Result<Id> {
        let id = self.backend.put(node).await?;
        self.head = id.clone();
        Ok(id)
    }

    /// Branch from the current head.
    ///
    /// This function creates a new AsyncDag object with the same backend (thus the backend must be
    /// `Clone` in this case).
    pub fn branch(&self) -> AsyncDag<Id, N, Backend>
    where
        Backend: Clone,
    {
        AsyncDag {
            head: self.head.clone(),
            backend: self.backend.clone(),
            _node: std::marker::PhantomData,
        }
    }

    /// Merge another AsyncDag into this one
    ///
    /// Use the `merger` function to merge the two head IDs and generate a new Node instance for
    /// the new HEAD of `self`.
    pub async fn merge<M>(&mut self, other: &AsyncDag<Id, N, Backend>, merger: M) -> Result<Id>
    where
        M: Merger<Id, N>,
    {
        let node = merger.create_merge_node(&self.head, &other.head)?;
        let id = self.backend.put(node).await?;
        self.head = id.clone();
        Ok(id)
    }

    pub fn rebase(&mut self) -> crate::Rebase<'_, Id, N, Backend> {
        crate::rebase::Rebase(self)
    }
}

pub trait Merger<Id, N>
where
    Id: NodeId,
    N: Node<Id = Id>,
{
    fn create_merge_node(&self, left_id: &Id, right_id: &Id) -> Result<N>;
}

impl<Id, N, F> Merger<Id, N> for F
where
    Id: NodeId,
    N: Node<Id = Id>,
    F: Fn(&Id, &Id) -> Result<N>,
{
    fn create_merge_node(&self, left_id: &Id, right_id: &Id) -> Result<N> {
        (self)(left_id, right_id)
    }
}

/// Stream adapter for streaming all nodes in a DAG
pub struct Stream<'a, Id, N, Backend>
where
    Id: NodeId + Send,
    N: Node<Id = Id>,
    Backend: DagBackend<Id, N>,
{
    dag: &'a AsyncDag<Id, N, Backend>,
    backlog: Vec<Pin<Backlog<'a, Id, N>>>,
}

pub type Backlog<'a, Id, N> =
    Box<(dyn futures::future::Future<Output = Result<Option<(Id, N)>>> + std::marker::Send + 'a)>;

impl<'a, Id, N, Backend> futures::stream::Stream for Stream<'a, Id, N, Backend>
where
    Id: NodeId + Send,
    N: Node<Id = Id>,
    Backend: DagBackend<Id, N>,
{
    type Item = Result<(Id, N)>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut futures::task::Context<'_>,
    ) -> futures::task::Poll<Option<Self::Item>> {
        if let Some(mut fut) = self.as_mut().backlog.pop() {
            match fut.as_mut().poll(cx) {
                Poll::Ready(Err(e)) => Poll::Ready(Some(Err(e))),
                Poll::Ready(Ok(Some((node_id, node)))) => {
                    for parent in node.parent_ids().into_iter() {
                        let fut = self.dag.backend.get(parent.clone());
                        self.as_mut().backlog.push(fut);
                    }
                    Poll::Ready(Some(Ok((node_id, node))))
                }
                Poll::Ready(Ok(None)) => {
                    // backend.get() returned Ok(None), so the referenced node seems not to exist
                    //
                    // TODO: Decide whether we should return an error here.
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                Poll::Pending => {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            }
        } else {
            Poll::Ready(None)
        }
    }
}

#[cfg(test)]
mod tests {

    use anyhow::Result;
    use futures::StreamExt;

    use crate::test_impl as test;
    use crate::AsyncDag;
    use crate::DagBackend;

    #[test]
    fn test_dag_two_nodes() {
        let head = test::Node {
            parents: vec![test::Id(0)],
            data: 1,
        };

        let b = test::Backend::new(vec![
            {
                Some(test::Node {
                    parents: vec![],
                    data: 0,
                })
            },
            { Some(head.clone()) },
        ]);

        {
            let node = tokio_test::block_on(b.get(test::Id(1))).unwrap().unwrap();
            assert_eq!(node.1.data, 1);
            assert!(!node.1.parents.is_empty()); // to check whether the parent is set
        }

        let dag = tokio_test::block_on(AsyncDag::new(b, head));
        assert!(dag.is_ok());
        let dag = dag.unwrap();

        {
            let has_id = tokio_test::block_on(dag.has_id(&test::Id(0)));
            assert!(has_id.is_ok());
            let has_id = has_id.unwrap();
            assert!(has_id);
        }
        {
            let has_id = tokio_test::block_on(dag.has_id(&test::Id(1)));
            assert!(has_id.is_ok());
            let has_id = has_id.unwrap();
            assert!(has_id);
        }
    }

    #[test]
    fn test_dag_two_nodes_stream() {
        let head = test::Node {
            parents: vec![test::Id(0)],
            data: 1,
        };

        let b = test::Backend::new(vec![
            {
                Some(test::Node {
                    parents: vec![],
                    data: 0,
                })
            },
            { Some(head.clone()) },
        ]);

        let dag = tokio_test::block_on(AsyncDag::new(b, head));
        assert!(dag.is_ok());
        let dag = dag.unwrap();

        let v = tokio_test::block_on(dag.stream().collect::<Vec<_>>());

        assert_eq!(v.len(), 2, "Expected two nodes: {:?}", v);
        assert_eq!(v[0].as_ref().unwrap().0, test::Id(1));
        assert_eq!(v[1].as_ref().unwrap().0, test::Id(0));
    }

    #[test]
    fn test_adding_head() {
        let head = test::Node {
            parents: vec![],
            data: 0,
        };
        let b = test::Backend::new(vec![Some(head.clone())]);

        let dag = tokio_test::block_on(AsyncDag::new(b, head));
        assert!(dag.is_ok());
        let mut dag = dag.unwrap();

        let new_head = test::Node {
            parents: vec![test::Id(0)],
            data: 1,
        };

        assert_eq!(dag.backend.0.read().unwrap().len(), 1);
        assert_eq!(dag.head, test::Id(0));

        let id = tokio_test::block_on(dag.update_head(new_head));
        assert!(id.is_ok());
        let _ = id.unwrap();

        assert_eq!(dag.backend.0.read().unwrap().len(), 2);
        assert_eq!(dag.head, test::Id(1));

        assert_eq!(dag.backend.0.read().unwrap()[0].as_ref().unwrap().data, 0);
        assert!(dag.backend.0.read().unwrap()[0]
            .as_ref()
            .unwrap()
            .parents
            .is_empty());

        assert_eq!(dag.backend.0.read().unwrap()[1].as_ref().unwrap().data, 1);
        assert_eq!(
            dag.backend.0.read().unwrap()[1]
                .as_ref()
                .unwrap()
                .parents
                .len(),
            1
        );
        assert_eq!(
            dag.backend.0.read().unwrap()[1].as_ref().unwrap().parents[0],
            test::Id(0)
        );
    }

    #[test]
    fn test_branch() {
        let mut dag = {
            let head = test::Node {
                parents: vec![],
                data: 0,
            };
            let b = test::Backend::new(vec![Some(head.clone())]);
            let dag = tokio_test::block_on(AsyncDag::new(b, head));
            assert!(dag.is_ok());
            dag.unwrap()
        };

        let branched = dag.branch();

        {
            assert_eq!(dag.backend.0.read().unwrap().len(), 1);
            assert_eq!(dag.head, test::Id(0));
            let new_head = test::Node {
                parents: vec![test::Id(0)],
                data: 1,
            };

            let id = tokio_test::block_on(dag.update_head(new_head));
            assert!(id.is_ok());
            let _ = id.unwrap();

            assert_eq!(dag.backend.0.read().unwrap().len(), 2);
            assert_eq!(dag.head, test::Id(1));
        }

        assert_eq!(branched.backend.0.read().unwrap().len(), 2);
        assert_eq!(branched.head, test::Id(0));
    }

    #[test]
    fn test_merging() {
        let mut dag = {
            let head = test::Node {
                parents: vec![],
                data: 0,
            };
            let b = test::Backend::new(vec![Some(head.clone())]);
            let dag = tokio_test::block_on(AsyncDag::new(b, head));
            assert!(dag.is_ok());
            dag.unwrap()
        };

        let mut branched = dag.branch();

        {
            assert_eq!(dag.backend.0.read().unwrap().len(), 1);
            assert_eq!(dag.head, test::Id(0));
            let new_head = test::Node {
                parents: vec![test::Id(0)],
                data: 1,
            };

            let id = tokio_test::block_on(dag.update_head(new_head));
            assert!(id.is_ok());
            let _ = id.unwrap();

            assert_eq!(dag.backend.0.read().unwrap().len(), 2);
            assert_eq!(dag.head, test::Id(1));
        }

        {
            assert_eq!(branched.backend.0.read().unwrap().len(), 2);
            assert_eq!(branched.head, test::Id(0));
            let new_head = test::Node {
                parents: vec![test::Id(0)],
                data: 2,
            };

            let id = tokio_test::block_on(branched.update_head(new_head));
            assert!(id.is_ok());
            let _ = id.unwrap();

            assert_eq!(branched.backend.0.read().unwrap().len(), 3);
            assert_eq!(branched.head, test::Id(2));
        }

        struct M;
        impl super::Merger<test::Id, test::Node> for M {
            fn create_merge_node(
                &self,
                left_id: &test::Id,
                right_id: &test::Id,
            ) -> Result<test::Node> {
                Ok(test::Node {
                    parents: vec![*left_id, *right_id],
                    data: 3,
                })
            }
        }

        let merge = tokio_test::block_on(dag.merge(&branched, M));
        assert!(merge.is_ok());
        let _ = merge.unwrap();

        assert_eq!(dag.backend.0.read().unwrap().len(), 4);
        assert_eq!(dag.head, test::Id(3));
    }

    #[test]
    fn test_merging_merge_fn() {
        let mut dag = {
            let head = test::Node {
                parents: vec![],
                data: 0,
            };
            let b = test::Backend::new(vec![Some(head.clone())]);
            let dag = tokio_test::block_on(AsyncDag::new(b, head));
            assert!(dag.is_ok());
            dag.unwrap()
        };

        let mut branched = dag.branch();

        {
            assert_eq!(dag.backend.0.read().unwrap().len(), 1);
            assert_eq!(dag.head, test::Id(0));
            let new_head = test::Node {
                parents: vec![test::Id(0)],
                data: 1,
            };

            let id = tokio_test::block_on(dag.update_head(new_head));
            assert!(id.is_ok());
            let _ = id.unwrap();

            assert_eq!(dag.backend.0.read().unwrap().len(), 2);
            assert_eq!(dag.head, test::Id(1));
        }

        {
            assert_eq!(branched.backend.0.read().unwrap().len(), 2);
            assert_eq!(branched.head, test::Id(0));
            let new_head = test::Node {
                parents: vec![test::Id(0)],
                data: 2,
            };

            let id = tokio_test::block_on(branched.update_head(new_head));
            assert!(id.is_ok());
            let _ = id.unwrap();

            assert_eq!(branched.backend.0.read().unwrap().len(), 3);
            assert_eq!(branched.head, test::Id(2));
        }

        let merge = tokio_test::block_on(dag.merge(
            &branched,
            |left_id: &test::Id, right_id: &test::Id| {
                Ok(test::Node {
                    parents: vec![*left_id, *right_id],
                    data: 3,
                })
            },
        ));
        assert!(merge.is_ok());
        let _ = merge.unwrap();

        assert_eq!(dag.backend.0.read().unwrap().len(), 4);
        assert_eq!(dag.head, test::Id(3));
    }

    #[test]
    fn test_rewrite_nodes() {
        let mut dag = {
            let head = test::Node {
                parents: vec![test::Id(3)],
                data: 4,
            };
            let b = test::Backend::new(vec![
                test::Node {
                    parents: vec![],
                    data: 0,
                },
                test::Node {
                    parents: vec![test::Id(0)],
                    data: 1,
                },
                test::Node {
                    parents: vec![test::Id(1)],
                    data: 2,
                },
                test::Node {
                    parents: vec![test::Id(2)],
                    data: 3,
                },
                {
                    head.clone()
                }
            ].into_iter().map(Some).collect());

            let dag = tokio_test::block_on(AsyncDag::new(b, head));
            assert!(dag.is_ok());
            dag.unwrap()
        };
        assert_eq!(dag.backend.0.read().unwrap().len(), 5);
        assert_eq!(dag.head, test::Id(4));

        let rebase = dag.stream()
            .take_while(|node| {
                match node {
                    Ok((id, _node)) => futures::future::ready(id.0 > 1), // the one we rebase on
                    Err(_) => unreachable!(), // in tests this should be unreachable
                }
            })
            .map(|node| {
                let (_id, node) = node.unwrap(); // should never fail, same as above
                test::Node {
                    parents: vec![if node.parents[0] == test::Id(1) {
                        test::Id(1) // base we rebase on
                    } else {
                        test::Id(node.parents[0].0 + 10)
                    }],
                    data: node.data + 10,
                }
            })
            .collect::<Vec<_>>();

        let rebase = tokio_test::block_on(rebase);
        for node in rebase.into_iter().rev() { // reverse order here, because we apply from oldest in chain to newest
            tokio_test::block_on(dag.update_head_unchecked(node)).unwrap();
        }

        // 15 because how the backend behaves
        assert_eq!(dag.backend.0.read().unwrap().len(), 15);

        // actual nodes:
        //  * 5 original: 0 <- 1 <- 2 <- 3 <- 4
        //  * 3 rewritten: 0 <- 1 <- 12 <- 13 <- 14
        assert_eq!(dag.backend.0.read().unwrap().iter().filter(|n| n.is_some()).count(), 5 + 3);
        assert_eq!(dag.head, test::Id(14));

        {
            let backend: &Vec<_> = &dag.backend.0.read().unwrap();

            // test NONE <- 0
            assert_eq!(backend.iter().filter_map(|n| n.as_ref()).find(|n| n.data == 0).unwrap().parents.len(), 0);

            // test 0 <- 1
            assert_eq!(backend.iter().filter_map(|n| n.as_ref()).find(|n| n.data == 1).unwrap().parents.len(), 1);
            assert_eq!(backend.iter().filter_map(|n| n.as_ref()).find(|n| n.data == 1).unwrap().parents[0], test::Id(0));

            // test 1 <- 12
            assert_eq!(backend.iter().filter_map(|n| n.as_ref()).find(|n| n.data == 12).unwrap().parents.len(), 1);
            assert_eq!(backend.iter().filter_map(|n| n.as_ref()).find(|n| n.data == 12).unwrap().parents[0], test::Id(1));

            // test 12 <- 13
            assert_eq!(backend.iter().filter_map(|n| n.as_ref()).find(|n| n.data == 13).unwrap().parents.len(), 1);
            assert_eq!(backend.iter().filter_map(|n| n.as_ref()).find(|n| n.data == 13).unwrap().parents[0], test::Id(12));

            // test 13 <- 14
            assert_eq!(backend.iter().filter_map(|n| n.as_ref()).find(|n| n.data == 14).unwrap().parents.len(), 1);
            assert_eq!(backend.iter().filter_map(|n| n.as_ref()).find(|n| n.data == 14).unwrap().parents[0], test::Id(13));
        }
    }

}
