'use strict';

const path = require('node:path');
const crypto = require('node:crypto');
const compare = (a, b) => a < b ? -1 : a > b ? 1 : 0;

function fail(message, code) {
  process.stdout.write(JSON.stringify({ schema: 'goya.manuscrito.graph/1', status: 'failed', diagnostics: [{ code, message }] }));
  process.exit(0);
}

const root = path.resolve(process.argv[1] || '.');
let ts;
let compilerPath;
try {
  compilerPath = process.env.MANUSCRITO_TYPESCRIPT_PATH;
  if (!compilerPath) fail('MANUSCRITO_TYPESCRIPT_PATH must point to a trusted TypeScript 5.4.5 installation; project dependencies are never loaded as compiler code', 'MAN-FE001');
  ts = require(path.resolve(compilerPath));
} catch {
  fail('could not load the configured trusted TypeScript compiler; expected version 5.4.5', 'MAN-FE001');
}
if (ts.version !== '5.4.5') {
  fail(`TypeScript frontend version mismatch: expected 5.4.5, found ${ts.version}`, 'MAN-FE002');
}

const fs = require('node:fs');
const compilerRoot = path.dirname(path.resolve(compilerPath));
const within = (base, candidate) => {
  const relative = path.relative(base, candidate);
  return relative === '' || (!relative.startsWith('..' + path.sep) && relative !== '..' && !path.isAbsolute(relative));
};
const allowedPath = (candidate) => {
  try {
    const real = fs.realpathSync.native(candidate);
    return within(root, real) || within(compilerRoot, real);
  } catch {
    const absolute = path.resolve(candidate);
    return within(root, absolute) || within(compilerRoot, absolute);
  }
};
const safeSystem = Object.create(ts.sys);
safeSystem.fileExists = (file) => allowedPath(file) && ts.sys.fileExists(file);
safeSystem.readFile = (file, encoding) => allowedPath(file) ? ts.sys.readFile(file, encoding) : undefined;
safeSystem.directoryExists = (dir) => allowedPath(dir) && ts.sys.directoryExists(dir);
safeSystem.getDirectories = (dir) => allowedPath(dir) ? ts.sys.getDirectories(dir).filter(allowedPath) : [];
safeSystem.readDirectory = (...args) => ts.sys.readDirectory(...args).filter(allowedPath);
safeSystem.realpath = (file) => allowedPath(file) ? ts.sys.realpath(file) : file;

const configPath = ts.findConfigFile(root, safeSystem.fileExists, 'tsconfig.json');
if (!configPath) fail('tsconfig.json not found; compiler options cannot be resolved', 'MAN-FE003');
const read = ts.readConfigFile(configPath, safeSystem.readFile);
if (read.error) fail(ts.flattenDiagnosticMessageText(read.error.messageText, '\n'), 'MAN-FE004');
const parsed = ts.parseJsonConfigFileContent(read.config, safeSystem, path.dirname(configPath), undefined, configPath);
const diagnostics = parsed.errors.map((d) => ({
  code: `TS${d.code}`,
  message: ts.flattenDiagnosticMessageText(d.messageText, '\n'),
  file: d.file ? path.relative(root, d.file.fileName).split(path.sep).join('/') : null,
  start: d.start ?? null,
}));
if (diagnostics.length) {
  process.stdout.write(JSON.stringify({ schema: 'goya.manuscrito.graph/1', status: 'failed', typescript_version: ts.version, diagnostics }));
  process.exit(0);
}

if ((parsed.projectReferences || []).some((ref) => {
  const target = path.resolve(path.dirname(configPath), ref.path);
  return !within(root, target) || !allowedPath(target);
})) {
  fail('project reference escapes the analyzed project root', 'MAN-FE005');
}
const host = ts.createCompilerHost(parsed.options);
const hostReadFile = host.readFile.bind(host);
const hostFileExists = host.fileExists.bind(host);
host.readFile = (file) => allowedPath(file) ? hostReadFile(file) : undefined;
host.fileExists = (file) => allowedPath(file) && hostFileExists(file);
host.realpath = (file) => {
  if (!allowedPath(file)) return file;
  try { return fs.realpathSync.native(file); } catch { return file; }
};
const program = ts.createProgram({ rootNames: parsed.fileNames, options: parsed.options, projectReferences: parsed.projectReferences, host });
const checker = program.getTypeChecker();
const sources = program.getSourceFiles().filter((sf) => {
  const relative = path.relative(root, sf.fileName);
  return relative && !relative.startsWith('..' + path.sep) && relative !== '..' &&
    !path.isAbsolute(relative) && !relative.split(path.sep).includes('node_modules');
}).sort((a, b) => compare(a.fileName, b.fileName));
const modules = [];
const edges = [];
const loaderBoundaries = [];
const symbols = [];
const mirNodes = [];
const capabilityObservations = [];
const sourceProvenance = [];
const sourceDigests = new Map();
const seen = new Set();
const addEdge = (source, node, specifier, kind) => {
  const key = `${source.fileName}\0${node.getStart(source)}\0${specifier}\0${kind}`;
  if (seen.has(key)) return;
  seen.add(key);
  const resolved = ts.resolveModuleName(specifier, source.fileName, parsed.options, safeSystem).resolvedModule;
  const packageId = resolved?.packageId;
  edges.push({
    source: path.relative(root, source.fileName).split(path.sep).join('/'),
    start: node.getStart(source), end: node.getEnd(), specifier, kind,
    resolved: resolved ? path.relative(root, resolved.resolvedFileName).split(path.sep).join('/') : null,
    external: resolved ? !!resolved.isExternalLibraryImport : null,
    package_name: packageId?.name || null,
    package_version: packageId?.version || null,
    unresolved: !resolved,
  });
};
const capabilityForModule = (specifier) => {
  const moduleName = specifier.replace(/^node:/, '');
  if (/^(fs|fs\/promises)$/.test(moduleName)) return 'filesystem';
  if (/^(child_process|process)$/.test(moduleName)) return 'processes';
  if (/^(http|https|http2|net|tls|dns|dgram)$/.test(moduleName)) return 'networking';
  if (moduleName === 'crypto') return 'cryptography';
  if (moduleName === 'os') return 'environment';
  if (/^(stream|stream\/promises)$/.test(moduleName)) return 'streams';
  if (/^(timers|timers\/promises)$/.test(moduleName)) return 'scheduling';
  if (/^(readline|readline\/promises|tty)$/.test(moduleName)) return 'terminal';
  if (moduleName === 'worker_threads') return 'workers';
  return null;
};

for (const sf of sources) {
  modules.push(path.relative(root, sf.fileName).split(path.sep).join('/'));
  const sourcePath = path.relative(root, sf.fileName).split(path.sep).join('/');
  const sourceDigest = `sha256:${crypto.createHash('sha256').update(sf.text, 'utf8').digest('hex')}`;
  sourceDigests.set(sourcePath, sourceDigest);
  const apiBindings = new Map();
  const createdRequireSymbols = new Set();
  const bindImport = (identifier, specifier, imported) => {
    const symbol = checker.getSymbolAtLocation(identifier);
    if (symbol) apiBindings.set(symbol, { specifier, imported });
  };
  const bindPattern = (name, specifier) => {
    if (ts.isIdentifier(name)) bindImport(name, specifier, '*');
    else if (ts.isObjectBindingPattern(name)) {
      for (const element of name.elements) {
        if (ts.isIdentifier(element.name)) {
          bindImport(element.name, specifier, element.propertyName && ts.isIdentifier(element.propertyName) ? element.propertyName.text : element.name.text);
        }
      }
    }
  };
  for (const statement of sf.statements) {
    if (ts.isImportDeclaration(statement) && ts.isStringLiteralLike(statement.moduleSpecifier) && statement.importClause) {
      const specifier = statement.moduleSpecifier.text;
      if (statement.importClause.name) bindImport(statement.importClause.name, specifier, '*');
      const bindings = statement.importClause.namedBindings;
      if (bindings && ts.isNamespaceImport(bindings)) bindImport(bindings.name, specifier, '*');
      if (bindings && ts.isNamedImports(bindings)) {
        for (const element of bindings.elements) bindImport(element.name, specifier, (element.propertyName || element.name).text);
      }
    }
  }
  const bindRuntimeLoads = (node) => {
    if (ts.isCallExpression(node) && node.arguments.length === 1 && ts.isStringLiteralLike(node.arguments[0]) &&
        ((ts.isIdentifier(node.expression) && node.expression.text === 'require') || node.expression.kind === ts.SyntaxKind.ImportKeyword)) {
      const declaration = ts.isVariableDeclaration(node.parent) && node.parent.initializer === node ? node.parent : null;
      if (declaration) bindPattern(declaration.name, node.arguments[0].text);
    }
    if (ts.isCallExpression(node) && ts.isIdentifier(node.expression) &&
        apiBindings.get(checker.getSymbolAtLocation(node.expression))?.specifier === 'node:module' &&
        apiBindings.get(checker.getSymbolAtLocation(node.expression))?.imported === 'createRequire' &&
        ts.isVariableDeclaration(node.parent) && node.parent.initializer === node && ts.isIdentifier(node.parent.name)) {
      const symbol = checker.getSymbolAtLocation(node.parent.name);
      if (symbol) createdRequireSymbols.add(symbol);
    }
    ts.forEachChild(node, bindRuntimeLoads);
  };
  bindRuntimeLoads(sf);
  const isNodeGlobal = (symbol) => symbol?.declarations?.some((decl) =>
    decl.getSourceFile().isDeclarationFile && /(^|[/\\])@types[/\\]node[/\\]/.test(decl.getSourceFile().fileName));
  const effectForInvocation = (node) => {
    let binding = null;
    let operation = null;
    if (ts.isIdentifier(node.expression)) {
      binding = apiBindings.get(checker.getSymbolAtLocation(node.expression)) || null;
      operation = binding?.imported;
    } else if (ts.isPropertyAccessExpression(node.expression) && ts.isIdentifier(node.expression.expression)) {
      binding = apiBindings.get(checker.getSymbolAtLocation(node.expression.expression)) || null;
      operation = node.expression.name.text;
    }
    if (!binding) return { effect: 'unclassified', capability: null, module: null, operation: node.expression.getText(sf) };
    const capability = capabilityForModule(binding.specifier);
    const allowed = {
      filesystem: new Set(['readFile','readFileSync','writeFile','writeFileSync','readdir','readdirSync','stat','lstat','open','rename','unlink','mkdir','rm','access','watch','copyFile','createReadStream','createWriteStream']),
      processes: new Set(['spawn','spawnSync','exec','execSync','execFile','execFileSync','fork']),
      networking: new Set(['createServer','request','get','connect','createConnection','createSocket','WebSocket']),
      cryptography: new Set(['createHash','createHmac','randomBytes','randomUUID','generateKeyPair','generateKeyPairSync','sign','verify','scrypt','pbkdf2','createCipheriv','createDecipheriv']),
      environment: new Set(['cpus','availableParallelism','homedir','hostname','platform','tmpdir','arch','release']),
      streams: new Set(['pipeline','finished']),
      scheduling: new Set(['setTimeout','setInterval','setImmediate','clearTimeout','clearInterval','clearImmediate','scheduler']),
      terminal: new Set(['createInterface','emitKeypressEvents']),
      workers: new Set(['Worker','MessageChannel','MessagePort']),
    };
    const recognized = capability && allowed[capability]?.has(operation);
    return {
      effect: recognized ? `${capability}.${operation}` : 'unclassified',
      capability: recognized ? capability : null, module: binding.specifier, operation,
    };
  };
  const addSymbol = (node, nameNode, kind, parameters = [], returnType = null, modifiers = node.modifiers) => {
    const symbolName = nameNode && (ts.isIdentifier(nameNode) || ts.isStringLiteralLike(nameNode)) ? nameNode.text : null;
    if (!symbolName) return;
    const type = checker.getTypeAtLocation(node);
    symbols.push({
      source: sourcePath, start: node.getStart(sf), end: node.getEnd(),
      name: symbolName, kind, exported: !!modifiers?.some((m) => m.kind === ts.SyntaxKind.ExportKeyword),
      type: checker.typeToString(type, node, ts.TypeFormatFlags.NoTruncation),
      parameters, return_type: returnType,
    });
  };
  for (const statement of sf.statements) {
    if (ts.isFunctionDeclaration(statement)) {
      const params = statement.parameters.map((p) => ({
        name: p.name.getText(sf),
        type: checker.typeToString(checker.getTypeAtLocation(p), p, ts.TypeFormatFlags.NoTruncation),
      }));
      const signature = checker.getSignatureFromDeclaration(statement);
      const ret = signature ? checker.typeToString(checker.getReturnTypeOfSignature(signature), statement, ts.TypeFormatFlags.NoTruncation) : null;
      addSymbol(statement, statement.name, 'function', params, ret);
    } else if (ts.isClassDeclaration(statement)) addSymbol(statement, statement.name, 'class');
    else if (ts.isInterfaceDeclaration(statement)) addSymbol(statement, statement.name, 'interface');
    else if (ts.isTypeAliasDeclaration(statement)) addSymbol(statement, statement.name, 'type-alias');
    else if (ts.isEnumDeclaration(statement)) addSymbol(statement, statement.name, 'enum');
    else if (ts.isVariableStatement(statement)) {
      for (const declaration of statement.declarationList.declarations) {
        const nameNode = declaration.name;
        if (ts.isIdentifier(nameNode)) addSymbol(declaration, nameNode, 'variable', [], null, statement.modifiers);
      }
    }
  }
  const emitMir = (node, parent, functionName) => {
    let kind = null;
    if (ts.isFunctionLike(node)) kind = 'function';
    else if (ts.isIfStatement(node) || ts.isConditionalExpression(node) || ts.isSwitchStatement(node)) kind = 'branch';
    else if (ts.isForStatement(node) || ts.isForInStatement(node) || ts.isForOfStatement(node) || ts.isWhileStatement(node) || ts.isDoStatement(node)) kind = 'iteration';
    else if (ts.isAwaitExpression(node)) kind = 'suspension';
    else if (ts.isThrowStatement(node)) kind = 'throw';
    else if (ts.isReturnStatement(node)) kind = 'return';
    else if (ts.isCallExpression(node) || ts.isNewExpression(node)) kind = 'invocation';
    else if (ts.isBinaryExpression(node) && node.operatorToken.kind >= ts.SyntaxKind.FirstAssignment && node.operatorToken.kind <= ts.SyntaxKind.LastAssignment) kind = 'mutation';
    else if (ts.isPrefixUnaryExpression(node) || ts.isPostfixUnaryExpression(node)) {
      if (node.operator === ts.SyntaxKind.PlusPlusToken || node.operator === ts.SyntaxKind.MinusMinusToken) kind = 'mutation';
    }
    else if (ts.isVariableDeclaration(node)) kind = 'binding';

    let nextParent = parent;
    let nextFunction = functionName;
    if (kind) {
      const id = `${sourcePath}#${node.getStart(sf)}:${kind}`;
      let fnName = functionName;
      if (kind === 'function') {
        if (node.name) fnName = node.name.getText(sf);
        else if (ts.isVariableDeclaration(node.parent) && ts.isIdentifier(node.parent.name)) fnName = node.parent.name.text;
        else if (ts.isPropertyDeclaration(node.parent) && node.parent.name) fnName = node.parent.name.getText(sf);
        else fnName = `<anonymous@${node.getStart(sf)}>`;
      }
      let inferredType = null;
      try { inferredType = checker.typeToString(checker.getTypeAtLocation(node), node, ts.TypeFormatFlags.NoTruncation); } catch { /* some syntax-only nodes have no type */ }
      const effectInfo = kind === 'invocation' ? effectForInvocation(node) : null;
      if (effectInfo) capabilityObservations.push({
        source: sourcePath, start: node.getStart(sf), end: node.getEnd(),
        module: effectInfo.module, capability: effectInfo.capability,
        operation: effectInfo.operation, status: effectInfo.capability ? 'recognized' : 'unclassified',
      });
      mirNodes.push({
        id, parent: parent || null, function: fnName || null, kind,
        source: sourcePath, start: node.getStart(sf), end: node.getEnd(),
        source_digest: sourceDigest, verification_status: 'unverified',
        transformation_history: ['typescript-ast-to-mir-node'],
        source_kind: ts.SyntaxKind[node.kind], inferred_type: inferredType,
        async: kind === 'function' ? !!node.modifiers?.some((m) => m.kind === ts.SyntaxKind.AsyncKeyword) : undefined,
        effects: effectInfo ? [effectInfo.effect] : [],
      });
      nextParent = id;
      nextFunction = fnName || functionName;
    }
    ts.forEachChild(node, (child) => emitMir(child, nextParent, nextFunction));
  };
  for (const statement of sf.statements) emitMir(statement, null, null);
  const visit = (node) => {
    if ((ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) && node.moduleSpecifier && ts.isStringLiteralLike(node.moduleSpecifier)) {
      addEdge(sf, node.moduleSpecifier, node.moduleSpecifier.text, ts.isImportDeclaration(node) ? 'import' : 're-export');
    } else if (ts.isCallExpression(node)) {
      const importCall = node.expression.kind === ts.SyntaxKind.ImportKeyword;
      const expressionSymbol = ts.isIdentifier(node.expression) ? checker.getSymbolAtLocation(node.expression) : null;
      const requireCall = ts.isIdentifier(node.expression) &&
        (createdRequireSymbols.has(expressionSymbol) || (node.expression.text === 'require' && isNodeGlobal(expressionSymbol)));
      const moduleRequireCall = ts.isPropertyAccessExpression(node.expression) && node.expression.name.text === 'require' &&
        ts.isIdentifier(node.expression.expression) && node.expression.expression.text === 'module' &&
        isNodeGlobal(checker.getSymbolAtLocation(node.expression.expression));
      if (importCall || requireCall || moduleRequireCall) {
        const kind = importCall ? 'dynamic-import' : (moduleRequireCall ? 'module-require' : 'require');
        const argument = node.arguments[0];
        if (argument && ts.isStringLiteralLike(argument)) addEdge(sf, argument, argument.text, kind);
        else loaderBoundaries.push({
          source: sourcePath, start: node.getStart(sf), end: node.getEnd(), kind,
          expression: argument ? argument.getText(sf) : null,
          reason: argument ? 'non-literal module specifier' : 'module loader has no static specifier',
        });
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(sf);
}
for (const source of modules) {
  sourceProvenance.push({
    source, source_digest: sourceDigests.get(source),
    symbols: symbols.filter((symbol) => symbol.source === source).map((symbol) => symbol.name),
    dependencies: edges.filter((edge) => edge.source === source).map((edge) => edge.specifier),
    verification_status: 'unverified',
    transformation_history: ['typescript-compiler-api-extraction'],
  });
}
edges.sort((a, b) => compare(a.source, b.source) || a.start - b.start || compare(a.specifier, b.specifier));
modules.sort();
symbols.sort((a, b) => compare(a.source, b.source) || a.start - b.start || compare(a.name, b.name));
mirNodes.sort((a, b) => compare(a.source, b.source) || a.start - b.start || compare(a.kind, b.kind));
capabilityObservations.sort((a, b) => compare(a.source, b.source) || a.start - b.start);
sourceProvenance.sort((a, b) => compare(a.source, b.source));
const typeDiagnostics = ts.getPreEmitDiagnostics(program).map((d) => ({
  code: `TS${d.code}`,
  message: ts.flattenDiagnosticMessageText(d.messageText, '\n'),
  file: d.file ? path.relative(root, d.file.fileName).split(path.sep).join('/') : null,
  start: d.start ?? null,
}));
const moduleSet = new Set(modules);
const adjacency = new Map(modules.map((module) => [module, []]));
for (const edge of edges) {
  if (!edge.unresolved && !edge.external && moduleSet.has(edge.source) && moduleSet.has(edge.resolved)) {
    adjacency.get(edge.source).push(edge.resolved);
  }
}
for (const targets of adjacency.values()) targets.sort(compare);
let nextIndex = 0;
const indexByModule = new Map();
const lowLinkByModule = new Map();
const stack = [];
const onStack = new Set();
const components = [];
const connect = (module) => {
  indexByModule.set(module, nextIndex);
  lowLinkByModule.set(module, nextIndex++);
  stack.push(module);
  onStack.add(module);
  for (const target of adjacency.get(module)) {
    if (!indexByModule.has(target)) {
      connect(target);
      lowLinkByModule.set(module, Math.min(lowLinkByModule.get(module), lowLinkByModule.get(target)));
    } else if (onStack.has(target)) {
      lowLinkByModule.set(module, Math.min(lowLinkByModule.get(module), indexByModule.get(target)));
    }
  }
  if (lowLinkByModule.get(module) === indexByModule.get(module)) {
    const component = [];
    let current;
    do {
      current = stack.pop();
      onStack.delete(current);
      component.push(current);
    } while (current !== module);
    component.sort(compare);
    components.push(component);
  }
};
for (const module of modules) if (!indexByModule.has(module)) connect(module);
components.sort((a, b) => compare(a[0], b[0]));
const cycles = components.filter((component) => component.length > 1 || adjacency.get(component[0]).includes(component[0]));
const declaredDependencies = {};
try {
  const manifestText = safeSystem.readFile(path.join(root, 'package.json'), 'utf8');
  const manifest = JSON.parse(manifestText);
  for (const group of ['dependencies', 'devDependencies', 'peerDependencies', 'optionalDependencies']) {
    Object.assign(declaredDependencies, manifest[group] || {});
  }
} catch { /* inventory report carries the missing/invalid manifest diagnostic */ }
const builtinModules = new Set(require('node:module').builtinModules.map((name) => name.replace(/^node:/, '')));
const packageNameFromSpecifier = (specifier) => {
  if (specifier.startsWith('node:') || builtinModules.has(specifier)) return null;
  const parts = specifier.split('/');
  return specifier.startsWith('@') ? parts.slice(0, 2).join('/') : parts[0];
};
const dependencyMap = new Map();
for (const edge of edges) {
  const name = edge.external ? (edge.package_name || packageNameFromSpecifier(edge.specifier)) : null;
  if (!name || name.startsWith('.') || name.startsWith('/')) continue;
  const existing = dependencyMap.get(name) || {
    name, resolved_version: edge.package_version,
    declared_version_constraint: declaredDependencies[name] || null,
    classification: 'unclassified', translation_status: 'not-translated',
    imports: [],
  };
  existing.imports.push({ source: edge.source, specifier: edge.specifier, resolved: edge.resolved });
  if (!existing.resolved_version && edge.package_version) existing.resolved_version = edge.package_version;
  dependencyMap.set(name, existing);
}
const reachableDependencies = [...dependencyMap.values()].sort((a, b) => compare(a.name, b.name));
for (const dependency of reachableDependencies) dependency.imports.sort((a, b) => compare(a.source, b.source) || compare(a.specifier, b.specifier));
const packageLockRaw = safeSystem.readFile(path.join(root, 'package-lock.json'), 'utf8');
let lockReport = { manager: 'npm', version: null, status: 'missing', source_digest: null, root_manifest_mismatches: [], integrity_issues_count: 0 };
const dependencyClosure = [];
const closureUnresolved = [];
if (packageLockRaw !== undefined) {
  lockReport.source_digest = `sha256:${crypto.createHash('sha256').update(packageLockRaw, 'utf8').digest('hex')}`;
  try {
    const lock = JSON.parse(packageLockRaw);
    lockReport.version = lock.lockfileVersion ?? null;
    if (![2, 3].includes(lock.lockfileVersion) || !lock.packages || typeof lock.packages !== 'object') {
      lockReport.status = 'unsupported';
    } else {
      lockReport.status = 'parsed';
      const packageEntries = lock.packages;
      const rootEntry = packageEntries[''] || {};
      const lockedRootConstraints = Object.assign({}, rootEntry.dependencies || {}, rootEntry.devDependencies || {}, rootEntry.peerDependencies || {}, rootEntry.optionalDependencies || {});
      lockReport.root_manifest_mismatches = Object.keys(declaredDependencies).sort(compare)
        .filter((name) => lockedRootConstraints[name] !== declaredDependencies[name])
        .map((name) => ({ name, manifest: declaredDependencies[name], lockfile: lockedRootConstraints[name] || null }));
      const lockEntryForResolved = (edge) => {
        if (!edge.resolved || !edge.package_name) return null;
        const resolvedParts = edge.resolved.split('/');
        const nameParts = edge.package_name.split('/');
        for (let i = resolvedParts.length - nameParts.length - 1; i >= 0; i--) {
          if (resolvedParts[i] !== 'node_modules') continue;
          if (nameParts.every((part, j) => resolvedParts[i + 1 + j] === part)) {
            const key = resolvedParts.slice(0, i + 1 + nameParts.length).join('/');
            const candidate = packageEntries[key];
            if (candidate && (!edge.package_version || candidate.version === edge.package_version)) return key;
          }
        }
        return null;
      };
      const resolveLockedDependency = (fromKey, name) => {
        let base = fromKey;
        while (true) {
          const candidate = [base, 'node_modules', name].filter(Boolean).join('/');
          if (packageEntries[candidate]) return candidate;
          const marker = base.lastIndexOf('/node_modules/');
          if (marker >= 0) base = base.slice(0, marker);
          else if (base.startsWith('node_modules/')) base = '';
          else return null;
        }
      };
      const queue = [];
      let queueIndex = 0;
      const visited = new Map();
      const visitedOrigins = new Set();
      for (const edge of edges) {
        if (!edge.external) continue;
        const packageName = edge.package_name || packageNameFromSpecifier(edge.specifier);
        if (!packageName) continue;
        const key = lockEntryForResolved({ ...edge, package_name: packageName });
        if (!key) {
          closureUnresolved.push({ name: packageName, version: edge.package_version, importer: edge.source, reason: 'resolved TypeScript package has no exact package-lock entry' });
          continue;
        }
        queue.push({ key, importer: edge.source, optional: false, peer: false });
      }
      while (queueIndex < queue.length) {
        const item = queue[queueIndex++];
        const entry = packageEntries[item.key];
        if (!entry) continue;
        let record = visited.get(item.key);
        if (!record) {
          const integrity = typeof entry.integrity === 'string' ? entry.integrity : null;
          const integrityParts = integrity ? integrity.trim().split(/\s+/) : [];
          const integrityValid = integrityParts.length > 0 && integrityParts.every((part) => /^(sha256|sha384|sha512)-[A-Za-z0-9+/]+={0,2}(\?.*)?$/.test(part));
          record = {
            name: item.key.slice(item.key.lastIndexOf('node_modules/') + 'node_modules/'.length),
            version: entry.version || null, install_path: item.key,
            integrity_present: !!integrity, integrity_format_valid: integrityValid,
            integrity, resolved: entry.resolved || null,
            link: entry.link === true, dev: entry.dev === true,
            optional: item.optional, peer: item.peer,
            classification: 'unclassified', reachable_from: new Set(),
          };
          visited.set(item.key, record);
          if (!integrityValid && !entry.link) lockReport.integrity_issues_count++;
        }
        record.reachable_from.add(item.importer);
        const originKey = `${item.key}\0${item.importer}`;
        if (visitedOrigins.has(originKey)) continue;
        visitedOrigins.add(originKey);
        if (entry.link) {
          closureUnresolved.push({ name: record.name, version: record.version, importer: item.importer, reason: 'workspace link requires workspace resolution' });
          continue;
        }
        const dependencyGroups = [
          [entry.dependencies || {}, false, false],
          [entry.optionalDependencies || {}, true, false],
          [entry.peerDependencies || {}, false, true],
        ];
        const handled = new Set();
        for (const [group, optional, peer] of dependencyGroups) {
          for (const name of Object.keys(group).sort(compare)) {
            if (handled.has(name)) continue;
            handled.add(name);
            const child = resolveLockedDependency(item.key, name);
            const optionalPeer = peer && entry.peerDependenciesMeta?.[name]?.optional === true;
            if (!child && optionalPeer) continue;
            if (!child) {
              closureUnresolved.push({ name, version: group[name], importer: item.key, reason: 'transitive dependency has no installed lock entry' });
            } else queue.push({ key: child, via: name, importer: item.importer, optional, peer });
          }
        }
      }
      dependencyClosure.push(...[...visited.values()].map((entry) => ({ ...entry, reachable_from: [...entry.reachable_from].sort(compare) })));
      dependencyClosure.sort((a, b) => compare(a.install_path, b.install_path));
    }
  } catch (error) {
    lockReport.status = 'invalid';
    lockReport.error = String(error.message || error);
  }
}
closureUnresolved.sort((a, b) => compare(a.importer, b.importer) || compare(a.name, b.name));
const compilerOptions = { ...parsed.options };
delete compilerOptions.configFilePath;
for (const key of ['rootDir', 'outDir', 'baseUrl', 'tsBuildInfoFile']) {
  if (typeof compilerOptions[key] === 'string' && path.isAbsolute(compilerOptions[key])) {
    compilerOptions[key] = path.relative(root, compilerOptions[key]).split(path.sep).join('/');
  }
}
process.stdout.write(JSON.stringify({
  schema: 'goya.manuscrito.graph/1', status: 'ok', typescript_version: ts.version,
  project_root: root, config_file: path.relative(root, configPath).split(path.sep).join('/'),
  compiler_options: compilerOptions,
  project_references: (parsed.projectReferences || []).map((ref) => ({ path: ref.path, prepend: !!ref.prepend })),
  modules, edges, loader_boundaries: loaderBoundaries, symbols, diagnostics, type_diagnostics: typeDiagnostics,
  mir: { version: 'goya.manuscrito.mir/1', status: 'extraction-only', nodes: mirNodes },
  capability_observations: capabilityObservations,
  source_provenance: sourceProvenance,
  reachable_dependencies: reachableDependencies,
  lockfile: lockReport,
  dependency_closure: dependencyClosure,
  dependency_closure_unresolved: closureUnresolved,
  strongly_connected_components: components,
  cycles,
  coverage: { resolved_edges: edges.filter((e) => !e.unresolved).length, unresolved_edges: edges.filter((e) => e.unresolved).length, total_edges: edges.length },
}));
