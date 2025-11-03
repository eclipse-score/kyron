# Kyron

**Kyron** is a customizable, high-performance `async/await` runtime designed for
advanced concurrent programming with focus on **funcional safety**
(https://en.wikipedia.org/wiki/ISO_26262). It allows fine-grained control over
scheduling, thread management, and workload isolation through configurable
execution engines.


## 🚀 Key Features

- **Multi-engine Runtime Architecture**
   Build a runtime composed of multiple, independently configured execution
   engines. Each engine can be tuned via the `ExecutionEngineBuilder` to meet
   specific workload requirements.

- **Configurable Threading Model**
  - Set **thread priorities** to control execution order
  - Define **CPU affinity** to pin async workers to specific cores for better
    cache locality
  - Add **dedicated workers** for specialized or blocking workloads without
    affecting async throughput

- **Work-Stealing Async Scheduler**
   Async workers operate under a **work-stealing model**, ensuring balanced task
   distribution and efficient CPU utilization. Tasks can migrate across workers
   dynamically for maximum throughput.

- **Dedicated Worker Locality**
   Dedicated workers guarantee **task locality** — a task spawned on a specific
   worker always resumes on that same thread, even after yielding. This enables
   deterministic execution behavior and predictable performance for thread-bound
   tasks.

- **IO event loop**
   Delivers IO event loop using operation system event multiplexing techniques

- **Net layer**
   Allows writing networking aplpications seamlessly

- **linux/qnx7.1/qnx8 support**


# 🛠️ Example

```rust
use kyron::prelude::*;
use kyron::runtime::RuntimeBuilder;

#[kyron::main(
     worker_threads = 4,
     worker_thread_parameters = {
        priority = 10,
        affinity = [0, 1],
    },
 )]
async fn main() {
    kyron::spawn(async {
        println!("Running inside Kyron async runtime!");
    })
    .await
    .unwrap();
}
```

# Feature status and roadmap
* [Async Runtime](src/kyron/doc/features.md)
