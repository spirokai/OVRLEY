---
name: implement
description: "Implement a piece of work based on a spec or set of tickets."
disable-model-invocation: true
---

Implement the work described by the user in the spec or tickets - do not extend beyond the scope of the ticket/issue.

Use /tdd where possible, at pre-agreed seams.

Run typechecking regularly, single test files regularly; do not run build or full test suite.

Do not use /code-review to review the work unless specifically requested by the user.

At the end, verify against acceptance criteria of ticket and update the status of ticket.
