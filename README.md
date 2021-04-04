# daglib

Async DAG library generic over Node type, Node Identifier Type and Backend
(storage of the DAG).


## Status

Experimental. Do not use.


## What is this?

My Problem was, that I wanted to implement a DAG on IPFS/IPLD. The Problem,
though, is that the existing IPFS/IPLD libraries do not feature functionality to
build a DAG easily.
There are libraries for DAGs, but they all implemented the storage functionality
themselves. I needed a backend that is _async_ and _generic_, so that I can use
IPFS (or anything else that works async) in the backend. So this lib was born.

This library does define a simple interface for the underlying backend:

```rust
#[async_trait]
pub trait DagBackend<Id, N>
    where N: Node,
          Id: NodeId + Send
{
    async fn get(&self, id: Id) -> Result<Option<N>>;
    async fn put(&mut self, node: N) -> Result<Id>;
}
```

and that's it. The `AsyncDag` type implements the DAG data structure on top of
that.


## Limitations

Because this library was initialized to be used with IPFS in the backend, is has
some limitations:

* It does assume that you cannot ever delete nodes. You can
  rewrite the DAG, but this does not necessarily delete nodes.

This list will be extended in the future. Do not assume it is complete!


## License

MPL-2.0
