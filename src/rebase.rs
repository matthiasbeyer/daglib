//! Module for the rebase types
//!
//! This module contains types for the `AsyncDag::rebase()` building of rebase functionality.
//!

use anyhow::Result;
use futures::StreamExt;

use crate::AsyncDag;
use crate::DagBackend;
use crate::Node;
use crate::NodeId;

pub struct Rebase<'a, Id, N, Backend>(pub(crate) &'a mut AsyncDag<Id, N, Backend>)
    where Id: NodeId + Send,
          N: Node<Id = Id>,
          Backend: DagBackend<Id, N>;

impl<'a, Id, N, Backend> Rebase<'a, Id, N, Backend>
    where Id: NodeId + Send,
          N: Node<Id = Id>,
          Backend: DagBackend<Id, N>
{
    pub fn onto(self, id: Id) -> RebaseOnto<'a, Id, N, Backend, IdEqualSelector<Id>> {
        RebaseOnto(self.0, id.clone(), IdEqualSelector(id))
    }
}

pub trait RebaseNodeRewriterCreator<'a, Id, N, Backend, Selector>
    where Id: NodeId + Send,
          N: Node<Id = Id>,
          Backend: DagBackend<Id, N>,
          Selector: IdSelector<Id>
{
    fn doing<Rewriter>(self, rewriter: Rewriter) -> RebaseNodeRewriter<'a, Id, N, Backend, Selector, Rewriter>
        where Rewriter: NodeRewriter<Id, N>;
}

pub struct RebaseOnto<'a, Id, N, Backend, Selector>(&'a mut AsyncDag<Id, N, Backend>, Id, Selector)
    where Id: NodeId + Send,
          N: Node<Id = Id>,
          Backend: DagBackend<Id, N>,
          Selector: IdSelector<Id>;

impl<'a, Id, N, Backend, Selector> RebaseNodeRewriterCreator<'a, Id, N, Backend, Selector> for RebaseOnto<'a, Id, N, Backend, Selector>
    where Id: NodeId + Send,
          N: Node<Id = Id>,
          Backend: DagBackend<Id, N>,
          Selector: IdSelector<Id>
{
    fn doing<Rewriter>(self, rewriter: Rewriter) -> RebaseNodeRewriter<'a, Id, N, Backend, Selector, Rewriter>
        where Rewriter: NodeRewriter<Id, N>
    {
        RebaseNodeRewriter {
            dag: self.0,
            base: self.1,
            selector: self.2,
            rewriter,
        }
    }
}

pub trait IdSelector<Id: NodeId> {
    fn select(&self, id: &Id) -> bool;
}

pub struct IdEqualSelector<Id: NodeId>(Id);
impl<Id: NodeId> IdSelector<Id> for IdEqualSelector<Id> {
    fn select(&self, id: &Id) -> bool {
        self.0 == *id
    }
}

pub trait NodeRewriter<Id: NodeId, N: Node<Id = Id>> {
    fn rewrite_node(&mut self, previous_id: Id, id: Id, node: N) -> Result<N>;
}

pub struct RebaseNodeRewriter<'a, Id, N, Backend, Selector, Rewriter>
    where Id: NodeId + Send,
          N: Node<Id = Id>,
          Backend: DagBackend<Id, N>,
          Selector: IdSelector<Id>,
          Rewriter: NodeRewriter<Id, N>,
{
    dag: &'a mut AsyncDag<Id, N, Backend>,
    base: Id,
    selector: Selector,
    rewriter: Rewriter,
}

impl<'a, Id, N, Backend, Selector, Rewriter> RebaseNodeRewriter<'a, Id, N, Backend, Selector, Rewriter>
    where Id: NodeId + Send,
          N: Node<Id = Id>,
          Backend: DagBackend<Id, N>,
          Selector: IdSelector<Id>,
          Rewriter: NodeRewriter<Id, N>
{
    pub async fn run(self) -> Result<Id> {
        let dag = self.dag;
        let sel = self.selector;
        let rew = self.rewriter;

        let v = dag.stream()
            .map(|node| {
                match node {
                    Ok((id, node)) => (!sel.select(&id), Ok((id, node))),
                    Err(e) => (true, Err(e)),
                }
            })
            .take_while(|tpl| futures::future::ready(tpl.0))
            .map(|tpl| tpl.1)
            .collect::<Vec<_>>()
            .await;

        futures::stream::iter(v.into_iter().rev())
            .fold(Ok((dag, rew, self.base)), |previous, r| async {
                match (previous, r) {
                    (Ok((dag, mut rew, prev)), Ok((id, node))) => {
                        let node = rew.rewrite_node(prev, id, node)?;
                        dag.update_head_unchecked(node)
                            .await
                            .map(|id| (dag, rew, id))
                    },
                    (_, Err(e)) => Err(e),
                    (Err(e), _) => Err(e),
                }
            })
            .await
            .map(|tpl| tpl.2)
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use futures::StreamExt;

    use crate::DagBackend;
    use crate::AsyncDag;
    use crate::test_impl as test;
    use crate::rebase::*;

    #[test]
    fn test_rebase() {
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

        struct Rewriter;

        impl crate::NodeRewriter<test::Id, test::Node> for Rewriter {
            fn rewrite_node(&mut self, previous_id: test::Id, id: test::Id, node: test::Node) -> Result<test::Node> {
                Ok({
                    test::Node {
                        parents: vec![if previous_id == test::Id(1) {
                            test::Id(1) // base we rebase on
                        } else {
                            test::Id(node.parents[0].0 + 10)
                        }],
                        data: node.data + 10,
                    }
                })
            }
        }

        let new_head = dag.rebase()
            .onto(test::Id(1))
            .doing(Rewriter)
            .run();

        let new_head = tokio_test::block_on(new_head);
        assert!(new_head.is_ok());
        let new_head = new_head.unwrap();

        // 15 because how the backend behaves
        assert_eq!(dag.backend.0.read().unwrap().len(), 15, "Expected 15 elements in backend: {:?}", dag.backend.0.read().unwrap());

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
