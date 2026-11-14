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
    host.setAttribute("aria-label", "Crates in view");
    $("layer-filters").after(host);
  }
  host.innerHTML = visibleCrates().map((crate) => `
    <button class="chip" type="button" aria-pressed="${crate.id === state.selected ? "true" : "false"}" data-crate="${esc(crate.id)}">${esc(shortName(crate.id))}</button>
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

function placeColumn(items, x, width, height) {
  const boxH = 36;
  const gap = 8;
  const total = items.length === 0 ? 0 : items.length * boxH + (items.length - 1) * gap;
  const y0 = Math.max(36, (height - total) / 2);
  return items.map((item, index) => ({
    ...item,
    x,
    y: y0 + index * (boxH + gap),
    w: width,
    h: boxH,
  }));
}

function renderGraph() {
  const svg = $("crate-graph");
  const selected = state.model.crates.find((crate) => crate.id === state.selected);
  const links = neighbors(state.selected);
  const uses = links.uses.map((id) => state.model.crates.find((crate) => crate.id === id)).filter(Boolean);
  const used = links.used.map((id) => state.model.crates.find((crate) => crate.id === id)).filter(Boolean);
  const width = 920;
  const boxH = 36;
  const gap = 8;
  const rows = Math.max(uses.length, used.length, 1);
  const height = Math.max(220, 72 + rows * (boxH + gap));
  const left = placeColumn(uses, 16, 250, height);
  const right = placeColumn(used, 654, 250, height);
  const center = {
    ...selected,
    x: 318,
    y: height / 2 - 32,
    w: 284,
    h: 64,
  };
  const nodes = left.concat([center], right);
  const edgeMarkup = left.concat(right).map((node) => {
    const fromRight = node.x < center.x;
    const x1 = fromRight ? node.x + node.w : center.x + center.w;
    const x2 = fromRight ? center.x : node.x;
    const y1 = node.y + node.h / 2;
    const y2 = center.y + center.h / 2;
    const mid = (x1 + x2) / 2;
    const stroke = fromRight ? "#9ad8c8" : "#e4b56a";
    return `<path d="M ${x1} ${y1} C ${mid} ${y1}, ${mid} ${y2}, ${x2} ${y2}" fill="none" stroke="${stroke}" stroke-width="1.4"></path>`;
  }).join("");
  const headings = `<text x="16" y="22" fill="#8a8175" font-size="11">USES</text><text x="654" y="22" fill="#8a8175" font-size="11">USED BY</text>`;
  const nodeMarkup = nodes.map((node) => {
    const active = node.id === state.selected;
    const fill = active ? "#234840" : "#221f1a";
    const stroke = active ? "#9ad8c8" : "#3d372f";
    const titleY = active ? node.y + 28 : node.y + 23;
    return `<g data-crate="${esc(node.id)}" tabindex="0" role="button" aria-label="${esc(node.id)}, ${esc(node.status)}" aria-pressed="${active ? "true" : "false"}">
      <rect x="${node.x}" y="${node.y}" width="${node.w}" height="${node.h}" rx="2" fill="${fill}" stroke="${stroke}"></rect>
      <text x="${node.x + 10}" y="${titleY}" fill="#f3eee4" font-size="13">${esc(node.id)}</text>
      ${active ? `<text x="${node.x + 10}" y="${node.y + 48}" fill="#9ad8c8" font-size="11">${esc(node.contract)} · ${esc(node.status)}</text>` : ""}
    </g>`;
  }).join("");
  const empty = (text, x) => `<text x="${x}" y="${height / 2}" fill="#8a8175" font-size="12">${text}</text>`;
  const empties = `${uses.length ? "" : empty("No workspace dependencies", 16)}${used.length ? "" : empty("No workspace dependents", 654)}`;
  svg.setAttribute("viewBox", `0 0 ${width} ${height}`);
  svg.setAttribute("width", String(width));
  svg.setAttribute("height", String(height));
  svg.innerHTML = `${headings}${edgeMarkup}${empties}${nodeMarkup}`;
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
  const caption = $("graph-caption");
  if (caption) {
    caption.textContent = `${selected.id} uses ${uses.length} workspace crates and is used by ${used.length}. Teal lines point inward. Amber lines point outward.`;
  }
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
  const requested = new URLSearchParams(location.search).get("event");
  await loadTrace(requested || state.trace);
}

main().catch((error) => {
  const mainEl = $("content");
  const note = document.createElement("p");
  note.textContent = `The review model did not load: ${error.message}`;
  mainEl.prepend(note);
});
