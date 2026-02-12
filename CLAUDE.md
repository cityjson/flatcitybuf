# Important Rules for LLM

## General Guidelines

- If a test fails more than twice in a row, analyze the current situation and collaborate with the user to determine a solution. Avoid trial-and-error testing without a hypothesis.
- The user has extensive knowledge gained from GitHub and can implement individual algorithms and libraries faster than you. Code should be written while explaining to the user, using test cases to verify correctness.
- When you work on the bigger tasks, you should plan first and document list of tasks to be done and update progress.md

### Core Files (Mandatory)

1. **`productContext.md`** [productContext.md](mdc:.llm/docs/productContext.md)

   - Explains the purpose of the project.
   - Identifies the problem it solves.
   - Describes expected functionality and user experience goals.

2. **`specification.md`** [specification.md](mdc:.llm/docs/specification.md)
   - Explains the detail of FlatCityBuf specification
   - Describes its encoding strategy and decisions made

3. **`projectStructure.md`** [projectStructure.md](mdc:.llm/docs/projectStructure.md)
   - Visual diagram of the project folder structure
   - Overview of all components and their relationships
   - Key patterns for workspace organization

### Additional Context Files

Additional files and folders can be created inside `.llm/docs/*` if they aid in organization:

- Documentation for complex features.
- Integration specifications.
- API documentation.
- Testing strategies.
- Deployment procedures.

---

## Integration with GitHub

you can expect that runtime has gh CLI, rather than using GitHub MCP, simply use gh CLI to create pull requests, etc.

---
