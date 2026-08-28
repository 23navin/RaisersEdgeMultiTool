Best Practices for Using the Query API to Synchronize Data to a Third-Party Database

Apply these best practices to use the Query application programming interface (API), including StartQueryExecutionJob and GetJob, to extract data and synchronize it with external systems. These practices help manage throttling behavior and include guidance shared by the Blackbaud SKY API team.
Query API execution model
The Query API runs requests as asynchronous background jobs. Each request enters a queue, runs when system capacity is available, and reports statuses throughout the job lifecycle. This model supports predictable synchronization and helps prevent throttling.

Job behavior

StartQueryExecutionJob queues a job for processing.
GetJob polls the job status until the system marks the job complete.
Jobs wait in the queue when the Query API reaches capacity limits.
A job can enter a Throttled state when system load prevents immediate execution.
Behavior by mode

Synchronous returns HTTP 429 when job limits are exceeded. Synchronous mode requires polling the job state at least every 10 seconds or the system cancels the job.
Asynchronous queues the job in a Throttled state when capacity is reached. Asynchronous mode does not require polling.
Submitting many queued jobs without monitoring their completion increases the likelihood of throttling, even when each request is valid.

Job queue management
Managing query jobs in a controlled sequence supports predictable performance and reduces the likelihood of throttling. When the system processes many active or throttled jobs at the same time, throughput decreases and pending jobs are more likely to enter throttled states.

Maintain a dedicated job queue for each user or integration account and submit the next job only after the current job reaches a terminal state, such as Completed or Failed. Blackbaud engineers report that throttling often occurs when clients submit several StartQueryExecutionJob requests before earlier jobs finish.

Use a consistent submit-and-wait pattern to support steady performance:

Submit a job.
Poll the system until it reaches a terminal state.
Retrieve results.
Submit the next job.
This pattern reduces load on the Query API and helps prevent repeated throttling when jobs compete for resources.

Throttled state behavior
A job enters the Throttled state when system capacity is unavailable. Throttled does not indicate failure; it means the job remains in the queue and will run when the system can process it. Throttled jobs complete when capacity frees up.

To support predictable performance, monitor the same job_id until it reaches a terminal state. Continuing to poll the existing job prevents unnecessary work and avoids creating replacement jobs that increase load. Using longer polling windows for throttled jobs gives the system more time to resume processing.

Job queue limits
Job queue limits affect how many throttled jobs the system can hold for each user. A user may have approximately 20 throttled jobs waiting in the queue at the same time. Environment-level throttling also applies, which means queued jobs can slow or pause when workload in the environment increases.

Throttling can occur even when a single user follows best practices. Other integrations, user-initiated queries, and concurrent workloads in the same environment contribute to shared capacity. Monitoring job behavior and understanding these shared limits help maintain predictable performance.

Important

If you exceed the throttled job queue, the system could reject new jobs with an HTTP 429 response when the user already has 20 throttled jobs.

Polling logic for realistic execution times
Jobs can remain queued or partially complete for long periods while they wait for available capacity. Because throttling is unpredictable, treat every throttled job as a long-running background process instead of a near-real-time operation.

Common partner issues include:

Polling that stops after five minutes because of fixed timeouts
Workloads that submit new jobs even though earlier throttled jobs remain in progress
Abandoned jobs that complete later and create inconsistent results
Recommended polling strategy

Design your polling logic to support long-running workloads and avoid fixed or narrow polling windows.

Use longer polling windows - Expect throttled jobs to continue for an indeterminate amount of time. A longer window helps to ensure that you continue to check the job until the system finishes processing it.
Track jobs persistently - Store the job identifier (ID) in your database. Persistent tracking helps to ensure that your service monitors the correct job and does not create duplicate requests while throttled work is still running.
Back off between polls - Use a back-off pattern to avoid unnecessary or excessive polling. A back-off delay reduces system load, supports fair throttling behavior, and improves overall completion rates.
Treat Query API jobs as batch workloads - Query API jobs function as batch workloads, not near-real-time endpoints. Design every workflow to handle asynchronous completion, potentially long durations, and variable processing times.
Query job scope and volume
Large jobs extend processing time and increase throttling risk. When you design smaller jobs that limit requests to the information your workflow requires, you reduce execution time and create more efficient and reliable processing patterns.

How to design efficient Query jobs

Use these guidelines when you define each job:

Limit requests to fields that your downstream workflow uses.
Submit jobs with smaller constituent counts.
Create several smaller sequential jobs instead of one large job.
Verify that each field identifier supports a required workflow purpose.
These practices reduce job duration, open background capacity more quickly, and decrease the risk of throttling events.

Throttling across shared environments
Throttling occurs because background capacity is shared across customers. A customer can experience throttling even when other customers do not and when their configuration has not changed. When you design workflows that adapt to shared capacity conditions, you improve reliability and maintain consistent performance.

How to build resilient sync logic

Use these guidelines when you design sync behavior:

Build sync logic that adapts to changes in shared background capacity.
Design workflows that do not depend on consistent execution timing across customers.
These practices create predictable processing patterns and support stable outcomes in shared environments.

Sync architecture for third-party database integrations
Use a sync architecture that supports predictable Query application programming interface (API) behavior across shared environments. A resilient design improves job sequencing, reduces throttling events, and produces consistent ingestion patterns.

Recommended sync model
Design your integration to use serialized execution, persistent tracking, adaptive polling, and clear operational visibility.

Serialized job execution - Run one active Query API job per user or integration. A single active job ensures consistent sequencing and prevents overlapping workloads that create unexpected throttling.

Persistent job tracking - Persistent tracking ensures that your service monitors the correct job and supports safe retries.
Track each job throughout its lifecycle to support reliable orchestration.
Store the following information:
Job identifier
Status
Submit time
Last poll time
Adaptive polling - Use polling behavior that responds to the job’s current status.
Use short polling intervals for Pending jobs.
Use slower polling intervals for Throttled jobs.
Use idempotent ingestion to safely reprocess results when polling overlaps or retries occur.
Operational visibility - Maintain insight into throttling behavior across customers.
Log throttling frequency per customer.
Alert on sustained throttling patterns to identify long-term capacity pressure.
Encouraging reliable Query API behavior
Design Query API workflows that support consistent execution across shared environments. When you use patterns that adapt to throttling and variable capacity, you create predictable job behavior and maintain stable performance.

How to promote stable job behavior and Query API use

Use these guidelines to maintain reliable Query API processing:

Design workflows for sequential batch behavior rather than high-concurrency extraction.
Recognize that background capacity is shared and that stable performance depends on cooperating with throttling behavior.
Wait for a throttled job to finish before you submit a new job.
Treat throttling as a temporary delay and continue monitoring job progress.
Use throttling signals to guide polling and sequencing.
Use the same user context because switching users does not reduce throttling in shared environments.
Use polling intervals that adjust to job status instead of relying on short, fixed timeouts.
Sequence Query API jobs so they do not run at the same time as heavy user-initiated queries.
These practices create stable ingestion patterns, reduce throttling pressure, and support predictable job orchestration.