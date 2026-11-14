const state = {
  model: null,
  layer: "all",
  selected: "goya-runtime",
  trace: "pointer-button",
  timer: 0,
};

const $ = (id) => document.getElementById(id);

function esc(value) {
  return String(value).replace(/[&<>"']/g, (ch) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  }[ch]));
}

function shortName(id) {
  return id.startsWith("goya-") ? id.slice(5) : id;
}

function revisionCopy(value) {
  if (value === "advanced") return "Revision advances. Layout runs.";
  if (value === "unchanged") return "Revision stays. Layout does not run.";
  if (value === "rejected") return "The classifier refuses the claim.";
  return "Outside the frame revision.";
}

function drawRuler() {
  const ticks = $("ruler-ticks");
  const parts = [];
  for (let i = 0; i <= 4; i += 1) {
    const x = 24 + i * 238;
    const label = (i * 25000).toString();
    parts.push(`<line x1="${x}" y1="14" x2="${x}" y2="22"></line>`);
    parts.push(`<text x="${x}" y="12" text-anchor="middle">${label}</text>`);
  }
  ticks.innerHTML = parts.join("");
}

function renderStats() {
  const model = state.model;
  const counts = model.crates.reduce((acc, item) => {
    acc[item.status] = (acc[item.status] || 0) + 1;
    return acc;
  }, {});
  $("version-line").textContent = `VERSION ${model.version_file} · workspace ${model.workspace_version}`;
  $("stats").innerHTML = [
    ["Crates", model.crates.length],
    ["Internal edges", model.edges.length],
    ["Partial or experimental", (counts.partial || 0) + (counts.experimental || 0)],
    ["Platform", "Wayland"],
  ].map(([label, value]) => `<div><dt>${esc(label)}</dt><dd>${esc(value)}</dd></div>`).join("");
}

function renderPaths() {
  $("path-grid").innerHTML = state.model.paths.map((path) => `
    <article class="path">
      <h3>${esc(path.title)}</h3>
      <p class="summary">${esc(path.summary)}</p>
      <div class="rack" role="group" aria-label="${esc(path.title)}">
        ${path.units.map((unit, index) => `
          <button class="unit" type="button" aria-pressed="${index === 0 ? "true" : "false"}">
            <span class="crate">${esc(unit.crate)}</span>
            <strong>${esc(unit.name)}</strong>
            <p>${esc(unit.detail)}</p>
          </button>
        `).join("")}
      </div>
    </article>
  `).join("");

  document.querySelectorAll(".rack").forEach((rack) => {
    rack.addEventListener("click", (event) => {
      const button = event.target.closest(".unit");
      if (!button) return;
      rack.querySelectorAll(".unit").forEach((unit) => {
        unit.setAttribute("aria-pressed", unit === button ? "true" : "false");
      });
    });
  });
}

function renderBounds() {
  $("bound-grid").innerHTML = state.model.boundaries.map((item) => `
    <article class="bound">
      <h3>${esc(item.name)}</h3>
      <div class="via">${esc(item.via)}</div>
      <p>${esc(item.note)}</p>
    </article>
  `).join("");
}

function renderFindings() {
  $("findings").innerHTML = state.model.findings.map((item) => `
    <article class="finding ${esc(item.kind)}">
      <div class="kind">${esc(item.kind)}</div>
      <h3>${esc(item.title)}</h3>
      <p>${esc(item.body)}</p>
    </article>
  `).join("");
}

function renderWaves() {
  $("wave-rail").innerHTML = state.model.waves.map((wave) => `
    <button class="wave-node ${esc(wave.status)}" type="button" data-wave="${esc(wave.id)}">
      <i></i>
      <span>${esc(wave.wave)}</span>
      <strong>${esc(wave.name)}</strong>
    </button>
  `).join("");
  $("wave-body").innerHTML = state.model.waves.map((wave) => `
    <tr id="wave-${esc(wave.id)}">
      <th scope="row">${esc(wave.wave)}</th>
      <td>${esc(wave.name)}</td>
      <td><span class="status ${esc(wave.status)}">${esc(wave.status)}</span></td>
      <td>${esc(wave.note)}</td>
    </tr>
  `).join("");
  $("wave-rail").addEventListener("click", (event) => {
    const button = event.target.closest(".wave-node");
    if (!button) return;
    const row = document.getElementById(`wave-${button.dataset.wave}`);
    document.querySelectorAll("#wave-body tr").forEach((tr) => tr.classList.remove("mark-row"));
    row.classList.add("mark-row");
    row.scrollIntoView({ block: "center" });
  });
}

function neighbors(id) {
  const used = [];
  const uses = [];
  state.model.edges.forEach((edge) => {
    if (edge.from === id) uses.push(edge.to);
    if (edge.to === id) used.push(edge.from);
  });
  return { used, uses };
}

function renderDetail() {
  const item = state.model.crates.find((crate) => crate.id === state.selected);
  if (!item) return;
  $("detail-name").textContent = item.id;
  $("detail-role").textContent = item.role;
  $("detail-meta").innerHTML = `
    <dt>Layer</dt><dd>${esc(item.layer)}</dd>
    <dt>Contract</dt><dd>${esc(item.contract)}</dd>
    <dt>Status</dt><dd><span class="status ${esc(item.status)}">${esc(item.status)}</span></dd>
  `;
  const links = neighbors(item.id);
  const list = (title, ids) => `
    <h4>${title}</h4>
    ${ids.length ? `<ul>${ids.map((id) => `<li><button class="text-button" type="button" data-crate="${esc(id)}">${esc(id)}</button></li>`).join("")}</ul>` : "<p>None in this workspace.</p>"}
  `;
  $("detail-links").innerHTML = `<div class="links">${list("Uses", links.uses)}${list("Used by", links.used)}</div>`;
  $("detail-links").querySelectorAll("button").forEach((button) => {
    button.addEventListener("click", () => selectCrate(button.dataset.crate));
  });
}

function selectCrate(id) {
  state.selected = id;
  const item = state.model.crates.find((crate) => crate.id === id);
  if (item && state.layer !== "all" && state.layer !== item.layer) {
    state.layer = item.layer;
    document.querySelectorAll(".filter").forEach((button) => {
      button.setAttribute("aria-pressed", button.dataset.layer === state.layer ? "true" : "false");
    });
  }
  renderDetail();
  renderGraph();
  renderChips();
}

function visibleCrates() {
  if (state.layer === "all") return state.model.crates;
  return state.model.crates.filter((crate) => crate.layer === state.layer);
}

function renderChips() {
  let host = $("crate-chips");
  if (!host) {
    host = document.createElement("div");
    host.id = "crate-chips";
    host.className = "chips";
    host.setAttribute("role", "listbox");
    host.setAttribute("aria-label", "Crates in view");
    $("layer-filters").after(host);
  }
  host.innerHTML = visibleCrates().map((crate) => `
    <button class="chip" type="button" role="option" aria-selected="${crate.id === state.selected ? "true" : "false"}" aria-pressed="${crate.id === state.selected ? "true" : "false"}" data-crate="${esc(crate.id)}">${esc(shortName(crate.id))}</button>
  `).join("");
  host.querySelectorAll(".chip").forEach((button) => {
    button.addEventListener("click", () => selectCrate(button.dataset.crate));
  });
}

function renderFilters() {
  const filters = [{ id: "all", title: "All layers" }].concat(state.model.layers);
  $("layer-filters").innerHTML = filters.map((layer) => `
    <button class="filter" type="button" data-layer="${esc(layer.id)}" aria-pressed="${layer.id === state.layer ? "true" : "false"}">${esc(layer.title)}</button>
  `).join("");
  $("layer-filters").addEventListener("click", (event) => {
    const button = event.target.closest(".filter");
    if (!button) return;
    state.layer = button.dataset.layer;
    $("layer-filters").querySelectorAll(".filter").forEach((item) => {
      item.setAttribute("aria-pressed", item === button ? "true" : "false");
    });
    const visible = visibleCrates();
    if (!visible.some((crate) => crate.id === state.selected) && visible[0]) {
      state.selected = visible[0].id;
      renderDetail();
    }
    renderChips();
    renderGraph();
  });
}

function renderGraph() {
  const svg = $("graph");
  const width = 980;
  const layers = state.layer === "all"
    ? state.model.layers
    : state.model.layers.filter((layer) => layer.id === state.layer);
  const rowH = 84;
  const height = 16 + layers.length * rowH;
  const nodes = [];
  layers.forEach((layer, row) => {
    const items = state.model.crates.filter((crate) => crate.layer === layer.id);
    const gap = 6;
    const inner = width - 24;
    const w = Math.max(78, Math.min(156, (inner - gap * Math.max(0, items.length - 1)) / Math.max(1, items.length)));
    const total = items.length * w + Math.max(0, items.length - 1) * gap;
    let x = 12 + (inner - total) / 2;
    const y = 18 + row * rowH;
    items.forEach((crate) => {
      nodes.push({ ...crate, x, y: y + 18, w, h: 44 });
      x += w + gap;
    });
  });
  const byId = new Map(nodes.map((node) => [node.id, node]));
  const linked = new Set();
  state.model.edges.forEach((edge) => {
    if (edge.from === state.selected || edge.to === state.selected) {
      linked.add(edge.from);
      linked.add(edge.to);
    }
  });
  const edges = state.model.edges.filter((edge) => byId.has(edge.from) && byId.has(edge.to) && (edge.from === state.selected || edge.to === state.selected));
  const layerLabels = layers.map((layer, row) => {
    const y = 30 + row * rowH;
    return `<text x="12" y="${y}" fill="#8a8175" font-size="11">${esc(layer.title)}</text>`;
  }).join("");
  const edgeMarkup = edges.map((edge) => {
    const from = byId.get(edge.from);
    const to = byId.get(edge.to);
    const x1 = from.x + from.w / 2;
    const y1 = from.y + from.h / 2;
    const x2 = to.x + to.w / 2;
    const y2 = to.y + to.h / 2;
    const mid = (y1 + y2) / 2;
    return `<path d="M ${x1} ${y1} C ${x1} ${mid}, ${x2} ${mid}, ${x2} ${y2}" fill="none" stroke="#9ad8c8" stroke-width="1.4" opacity="0.85"></path>`;
  }).join("");
  const nodeMarkup = nodes.map((node) => {
    const active = node.id === state.selected;
    const near = linked.has(node.id);
    const fill = active ? "#234840" : near ? "#3a3224" : "#221f1a";
    const stroke = active ? "#9ad8c8" : near ? "#e4b56a" : "#3d372f";
    return `<g data-crate="${esc(node.id)}" tabindex="0" role="button" aria-label="${esc(node.id)}, ${esc(node.status)}" aria-pressed="${active ? "true" : "false"}">
      <rect x="${node.x}" y="${node.y}" width="${node.w}" height="${node.h}" rx="2" fill="${fill}" stroke="${stroke}"></rect>
      <text x="${node.x + 8}" y="${node.y + 20}" fill="#f3eee4" font-size="11">${esc(shortName(node.id))}</text>
      <text x="${node.x + 8}" y="${node.y + 35}" fill="#8a8175" font-size="10">${esc(node.status)}</text>
    </g>`;
  }).join("");
  svg.setAttribute("viewBox", `0 0 ${width} ${height}`);
  svg.setAttribute("width", String(width));
  svg.setAttribute("height", String(height));
  svg.innerHTML = `${layerLabels}${edgeMarkup}${nodeMarkup}`;
  svg.querySelectorAll("g[data-crate]").forEach((node) => {
    const choose = () => selectCrate(node.dataset.crate);
    node.addEventListener("click", choose);
    node.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        choose();
      }
    });
  });
  const shown = nodes.length;
  $("graph-caption").textContent = `${shown} crates in view. Edges show what ${state.selected} uses and what uses it.`;
}

function renderTraceButtons(events) {
  $("trace-bar").innerHTML = events.map((event) => `
    <button type="button" data-event="${esc(event.id)}" aria-pressed="${event.id === state.trace ? "true" : "false"}">${esc(event.label)}</button>
  `).join("");
  $("trace-bar").addEventListener("click", (event) => {
    const button = event.target.closest("button");
    if (!button) return;
    loadTrace(button.dataset.event);
  });
}

function renderTrace(trace) {
  window.clearTimeout(state.timer);
  state.trace = trace.id;
  $("trace-bar").querySelectorAll("button").forEach((button) => {
    button.setAttribute("aria-pressed", button.dataset.event === trace.id ? "true" : "false");
  });
  $("trace-summary").textContent = trace.summary;
  const badge = $("revision");
  badge.className = `revision ${trace.revision}`;
  badge.textContent = revisionCopy(trace.revision);
  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  $("steps").innerHTML = trace.steps.map((step, index) => `
    <li class="step ${esc(step.state)} ${reduce ? "lit" : ""}" style="--i:${index}">
      <div class="n">${String(index + 1).padStart(2, "0")}</div>
      <div>
        <span class="state">${esc(step.state)}</span>
        <h3>${esc(step.title)}</h3>
        <div class="crate">${esc(step.crate)}</div>
        <p>${esc(step.detail)}</p>
      </div>
    </li>
  `).join("");
  if (reduce) return;
  const items = [...$("steps").children];
  const light = (index) => {
    if (index >= items.length) return;
    items[index].classList.add("lit");
    state.timer = window.setTimeout(() => light(index + 1), 140);
  };
  light(0);
}

async function loadTrace(id) {
  const response = await fetch(`/api/trace?event=${encodeURIComponent(id)}`);
  if (!response.ok) throw new Error(`trace ${response.status}`);
  renderTrace(await response.json());
}

async function main() {
  drawRuler();
  const response = await fetch("/api/model");
  if (!response.ok) throw new Error(`model ${response.status}`);
  state.model = await response.json();
  renderStats();
  renderPaths();
  renderBounds();
  renderFindings();
  renderWaves();
  renderFilters();
  renderDetail();
  renderChips();
  renderGraph();
  const catalog = await fetch("/api/trace");
  const events = await catalog.json();
  renderTraceButtons(events.events);
  await loadTrace(state.trace);
}

main().catch((error) => {
  const mainEl = $("content");
  const note = document.createElement("p");
  note.textContent = `The review model did not load: ${error.message}`;
  mainEl.prepend(note);
});
