# Routing

## Model routing table

| Job | Model | Effort | Why |
|-----|-------|--------|-----|
| PM / decomposition | Opus 5.5 | high | long-horizon, costly to get wrong |
| Interface & schema design | Opus 5.5 | xhigh | frozen contracts, hard to reverse |
| Implementer, spec'd | Sonnet 5.5 | high | bulk of work, 1/5 the output cost |
| Implementer, gnarly | Opus 5.5 | high | concurrency, migrations, security |
| Adversarial verifier | Opus 5.5 | high | must out-think the author |
| Second verifier / tiebreak | Fable 5.1 | high | ONLY irreversible or contested |
| Code review | Sonnet 5.5 | medium | pattern matching, not invention |
| Search / discovery | Haiku 5.5 | low | mechanical, high volume |
| Tests, logs, docs | Haiku 5.5 | low | verbose output, low stakes |

Escalate one tier only when: a verifier rejected it twice, it touches a frozen
interface, it is irreversible in production, or the failure mode is silent.
Downgrade when a story needed no clarification and passed first time. If you
cannot justify a tier above Sonnet, use Sonnet.

## Model reference

Verified 2026-11-03 against the claude-api skill model table.

| Model | ID | Price in/out (per Mtok) | Context | Default effort |
|-------|----|-------------------------|---------|----------------|
| Fable 5.1 | `claude-fable-5-1` | $10/$50 | 1M | high |
| Opus 5.5 | `claude-opus-5-5` | $4/$20 | 1M | medium |
| Sonnet 5.5 | `claude-sonnet-5-5` | $2/$10 | 1M | high |
| Haiku 5.5 | `claude-haiku-5-5` | $0.10/$0.50 | 1M | medium |

## Escalation log

| Date | Story | From | To | Trigger |
|------|-------|------|----|---------|

## Context log

| Date | Session | Tokens at checkpoint | End type | Compactions |
|------|---------|----------------------|----------|-------------|

## Session budgets

| Role | Budget |
|------|--------|
| PM | 150K tokens |
| Implementer | 120K tokens |
| Verifier | 80K tokens |
| Haiku roles | none |
