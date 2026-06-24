# Syntax Parsing

Tree-sitter is the tolerant syntax and candidate-generation layer. Its facts
must be converted to RepoGrammar-owned `CodeUnit` and IR types before entering
core, and structural syntax evidence alone must not prove family membership.
