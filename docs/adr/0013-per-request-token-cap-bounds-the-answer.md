# ADR 0013: The Per-Request Token Cap Bounds the Answer

- Status: Accepted
- Date: 2026-10-11

## Context

P5-004 froze a `maxTokensPerRequest` value into every Session, defaulting to 2,048, and P5-005 applied it as the ceiling of the whole request: prompt, transcript context, and answer together. The answer was then given a quarter of that.

The first real summary showed what that means. A ten-segment meeting of about 6 KB was summarized from three of its segments, because the request could hold roughly 1,500 tokens of input, and the summary had 512 tokens to say it in. Insight requests were cut the same way. Nothing had failed; the result was simply poor, and the setting a user would read as "how long can an answer be" was deciding how much of the meeting the model was allowed to see.

## Decision

`maxTokensPerRequest` bounds what the model may write — the `max_tokens` of the request, which is what the name means in every model API. How much of the meeting a request carries is the prompt specification's own `approximateRequestTokenLimit`, and the summary's is raised from 16,384 to 32,768 so a longer meeting fits before sampling starts.

- The prompt builder sets the answer ceiling to the smaller of the specification's maximum and the Session's cap, and the request ceiling to the specification's limit.
- The budget ledger checks the answer ceiling against the Session's cap, instead of the request ceiling.
- Cost stays bounded where it already was: every request is reserved at its full input and output ceiling against the Session's spending limit before it is sent.
- The Settings field is relabelled **Maximum answer length (tokens)** and says it does not limit how much of the meeting is read.

## Consequences

- Sessions already created keep their frozen value and now get a full context with up to that many tokens of answer. A summary regenerated for an existing meeting improves without editing the meeting.
- A request carries more text and so reserves more of the session budget. With the default cap and a typical inexpensive model this is a fraction of a cent; a session with a very small spending limit may now refuse a request it would have sent truncated.
- A model whose context window is smaller than a prompt's request limit (32,768 for summaries) is refused before sending with `usage_model_context_too_small`, as before.
