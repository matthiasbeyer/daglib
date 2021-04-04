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

