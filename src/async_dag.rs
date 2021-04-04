use std::pin::Pin;

use anyhow::Result;
use anyhow::anyhow;
use futures::stream::StreamExt;
use futures::task::Poll;

use crate::Node;
use crate::NodeId;
use crate::DagBackend;

/// An async DAG, generic over Node, Node identifier and Backend implementation
pub struct AsyncDag<Id, N, Backend>
    where Id: NodeId + Send,
          N: Node<Id = Id>,
          Backend: DagBackend<Id, N>
{
    head: N,
    backend: Backend,
}

impl<Id, N, Backend> AsyncDag<Id, N, Backend>
    where Id: NodeId + Send,
          N: Node<Id = Id>,
          Backend: DagBackend<Id, N>
{
    pub async fn new(backend: Backend, head: N) -> Result<Self> {
        backend
            .get(head.id().clone())
            .await?
            .map(|node| {
                AsyncDag {
                    head: node,
                    backend: backend
                }
            })
            .ok_or_else(|| anyhow!("Head not found in backend"))
    }

    pub async fn has_id(&self, id: &Id) -> Result<bool> {
        self.stream()
            .map(|r| -> Result<bool> {
                r.map(|node| node.id() == id)
            })
            .collect::<Vec<Result<bool>>>()
            .await
            .into_iter()
            .fold(Ok(false), |acc, e| {
                match (acc, e) {
                    (Err(e), _) => Err(e),
                    (Ok(_), Err(e)) => Err(e),
                    (Ok(a), Ok(b)) => Ok(a || b),
                }
            })
    }

    async fn get_next(&self, id: Id) -> Result<Vec<N>> {
        self.backend
            .get(id)
            .await?
            .ok_or_else(|| anyhow!("ID Not found"))?
            .parent_ids()
            .into_iter()
            .map(|id| async move {
                self.backend
                    .get(id)
                    .await
                    .transpose()
            })
            .collect::<futures::stream::FuturesUnordered<_>>()
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .filter_map(|o| o)
            .collect()
    }

    fn stream(&self) -> Stream<Id, N, Backend>  {
        Stream {
            dag: self,
            backlog: {
                let mut v = Vec::with_capacity(2);
                v.push(self.backend.get(self.head.id().clone()));
                v
            }
        }
    }

}


pub struct Stream<'a, Id, N, Backend>
    where Id: NodeId + Send,
          N: Node<Id = Id>,
          Backend: DagBackend<Id, N>
{
    dag: &'a AsyncDag<Id, N, Backend>,
    backlog: Vec<Pin<Box<(dyn futures::future::Future<Output = Result<Option<N>>> + std::marker::Send + 'a)>>>,
}

impl<'a, Id, N, Backend> futures::stream::Stream for Stream<'a, Id, N, Backend>
    where Id: NodeId + Send,
          N: Node<Id = Id>,
          Backend: DagBackend<Id, N>
{

    type Item = Result<N>;

    /// Attempt to resolve the next item in the stream.
    /// Returns `Poll::Pending` if not ready, `Poll::Ready(Some(x))` if a value
    /// is ready, and `Poll::Ready(None)` if the stream has completed.
    fn poll_next(mut self: std::pin::Pin<&mut Self>, cx: &mut futures::task::Context<'_>) -> futures::task::Poll<Option<Self::Item>> {
        if let Some(mut fut) = self.as_mut().backlog.pop() {
            match fut.as_mut().poll(cx) {
                Poll::Ready(Err(e)) => Poll::Ready(Some(Err(e))),
                Poll::Ready(Ok(Some(node))) => {
                    for parent in node.parent_ids().into_iter() {
                        let fut = self.dag.backend.get(parent);
                        self.as_mut().backlog.push(fut);
                    }
                    Poll::Ready(Some(Ok(node)))
                },
                Poll::Ready(Ok(None)) => {
                    // backend.get() returned Ok(None), so the referenced node seems not to exist
                    //
                    // TODO: Decide whether we should return an error here.
                    cx.waker().wake_by_ref();
                    Poll::Pending
                },
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
    use std::pin::Pin;

    use anyhow::Result;
    use anyhow::anyhow;
    use async_trait::async_trait;
    use tokio_test::block_on;

    use crate::DagBackend;
    use crate::AsyncDag;
    use crate::test_impl as test;

    #[test]
    fn test_dag_two_nodes() {
        let head = test::Node {
            id: test::Id(1),
            parents: vec![test::Id(0)],
            data: 43,
        };

        let b = test::Backend(vec![
            {
                Some(test::Node {
                    id: test::Id(0),
                    parents: vec![],
                    data: 42,
                })
            },
            {
                Some(head.clone())
            },
        ]);

        {
            let node = tokio_test::block_on(b.get(test::Id(1))).unwrap().unwrap();
            assert_eq!(node.data, 43);
            assert_eq!(node.id, test::Id(1));
            assert!(!node.parents.is_empty()); // to check whether the parent is set
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

        {
            let next = tokio_test::block_on(dag.get_next(test::Id(1)));
            assert!(next.is_ok());
            let mut next = next.unwrap();
            assert_eq!(next.len(), 1);
            let node = next.pop();
            assert!(node.is_some());
            let node = node.unwrap();
            assert_eq!(node.id, test::Id(0));
            assert_eq!(node.data, 42);
            assert!(node.parents.is_empty());
        }
    }
    #[test]
    fn test_dag_two_nodes_stream() {
        use futures::StreamExt;

        let head = test::Node {
            id: test::Id(1),
            parents: vec![test::Id(0)],
            data: 43,
        };

        let b = test::Backend(vec![
            {
                Some(test::Node {
                    id: test::Id(0),
                    parents: vec![],
                    data: 42,
                })
            },
            {
                Some(head.clone())
            },
        ]);

        let dag = tokio_test::block_on(AsyncDag::new(b, head));
        assert!(dag.is_ok());
        let dag = dag.unwrap();

        let v = tokio_test::block_on(dag.stream().collect::<Vec<_>>());

        assert_eq!(v.len(), 2, "Expected two nodes: {:?}", v);
        assert_eq!(v[0].as_ref().unwrap().id, test::Id(1));
        assert_eq!(v[1].as_ref().unwrap().id, test::Id(0));
    }

}

