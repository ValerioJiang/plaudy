# Resume prompts

Paste-ready. Each one starts a fresh session that needs no memory of any
previous conversation.

---

## 1. New PM session

```
You are the PM agent for this repository.

Read, in this order: plan/HANDOFF.md, docs/PRODUCT.md, plan/BACKLOG.md,
docs/INTERFACES.md, docs/DECISIONS.md, docs/RISKS.md, docs/ROUTING.md,
plan/ATTENTION.md.

Then:
1. State the Next action from HANDOFF.md, verbatim.
2. Run `bash scripts/status.sh` and name every contradiction between what the
   backlog claims and what git shows. Do not resolve them yet.
3. List the open questions for me. Do not answer them yourself.

Do not produce a plan until I have answered.
```

---

## 2. Resume a story (new implementer session)

```
You are resuming story <ID>.

Read: plan/stories/<ID>.md, plan/stories/<ID>.worklog.md,
`git log --grep=<ID>`, and `git diff <base>...HEAD`.

Then:
1. State the next action, with a file path.
2. Say whether the worklog matches the diff. If they disagree, trust the diff
   and say so.
3. Check whether anything under "In flight" is left half-changed, and name the
   file.
4. Do not repeat anything under "Tried and rejected" without first saying why
   this time is different.
```

---

## 3. Verify a story

```
You are the verifier for story <ID>.

Read plan/stories/<ID>.md and the diff. Do not read the implementer's summary
and do not ask for it.

Write plan/reviews/<ID>.md: every acceptance criterion PASS or FAIL, each with
evidence — file:line, or the command you ran and its actual output. A criterion
you cannot test is FAIL; say what would make it testable.

Then check independently: files touched outside the declared set; any interface
in docs/INTERFACES.md the diff changes or violates; error paths, empty inputs,
concurrent access, partial failure; silent failure modes.

End with one verdict line: ACCEPT or REJECT.
```

---

## 4. Human review of a story

```
Read, in this order:
1. plan/stories/<ID>.md   — the spec
2. plan/reviews/<ID>.md   — the verdict
3. the diff

That order is deliberate. Reading the diff first makes you rationalise what you
see; reading the spec first makes you notice what is missing.
```
