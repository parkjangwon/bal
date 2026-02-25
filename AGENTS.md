# bal Project Agent Guidelines

> This file contains the development philosophy and guidelines for AI agents working on the bal project.
> Read this before making any changes to understand the project's core values.

## Core Philosophy

### 1. Instantaneity (Zero Latency)
- **Delay is unacceptable**. 5 seconds → 200ms → even that's too long.
- Goal: **Immediate response** with zero perceived wait time.
- If a user has to wait, it's a bug.
- Failover must happen in milliseconds, not seconds.

### 2. Pragmatism Over Perfection
- **"Make it work first"** beats perfect architecture.
- Working code today > Perfect code tomorrow.
- Remove unnecessary comments, complexity, and ceremony.
- Prefer one-liner solutions (`curl | bash`) over multi-step processes.

### 3. Intuitive UX
- Commands should work **as expected**.
- `bal start` → foreground (see logs immediately)
- `bal start -d` → daemon (background)
- No surprises. No hidden magic.

### 4. Fast Feedback Loop
- Commit fast, tag fast, release fast.
- GitHub Actions for automated releases.
- Test with real behavior, not just unit tests.
- "Actually test it" - verify it works in practice.

### 5. Radical Simplicity
- One-line uninstall command.
- README shows only essential commands.
- Remove features before adding complexity.
- If it can be simpler, make it simpler.

---

## Development Guidelines

### When Adding Features
1. Does it make it faster? → Yes → Do it
2. Does it add complexity? → Yes → Question it
3. Is it intuitive? → No → Redesign it
4. Can it be one line? → Yes → Make it one line

### Code Style
- **Comments**: Only explain WHY, not WHAT.
- **Logging**: Debug level by default. Info for user-facing events.
- **Error handling**: Fail fast, fail clear.
- **Configuration**: Convention over configuration.

### Testing Philosophy
- Test real behavior, not mocks.
- Kill backends, restart them, measure recovery time.
- If it feels slow to a human, it's too slow.

### Release Process
1. Make changes
2. Build and test immediately
3. Commit with clear message
4. Tag and push
5. Let GitHub Actions handle the rest

---

## Anti-Patterns (Don't Do This)

❌ **Over-engineering**: Complex abstractions for simple problems  
❌ **Ceremony**: Boilerplate, unnecessary type wrappers  
❌ **Delayed feedback**: Long build times, manual release steps  
❌ **Surprise UX**: Commands that do unexpected things  
❌ **Verbose logging**: Logging that fills disk without value  

---

## Preferred Solutions

### Installation
```bash
# One line, no questions
curl -sSL https://raw.githubusercontent.com/parkjangwon/bal/main/install.sh | bash
```

### Uninstallation
```bash
# One line, force mode for pipes
curl -sSL https://raw.githubusercontent.com/parkjangwon/bal/main/install.sh | bash -s -- --uninstall
```

### Configuration
- Keep defaults sensible (port 9295)
- Minimal YAML, no unnecessary comments
- Load from ~/.bal/ automatically

---

## Performance Targets

| Metric | Target | Acceptable | Unacceptable |
|--------|--------|------------|--------------|
| Health check interval | 200ms | 500ms | 5s |
| Failover time | <100ms | <500ms | >1s |
| Backend recovery | Immediate | <200ms | >1s |
| Connection timeout | 500ms | 1s | 2s |

---

## Communication Style

When working with this codebase:

1. **Be direct**: "Done." is better than "I have successfully completed..."
2. **Show, don't tell**: Working code > explanations
3. **Fast iterations**: Small commits, frequent pushes
4. **Question complexity**: "Can this be simpler?"
5. **Test immediately**: Build it, run it, verify it

---

## Remember

> "The best code is no code. The second best is code that disappears."
> 
> "If it takes longer than 5 seconds, it's broken."
> 
> "Make it work, make it fast, make it simple. In that order."

---

*This file should be updated as the project evolves. The core philosophy stays: Fast, Simple, Intuitive.*
