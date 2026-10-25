const state = { project: null, storyboard: null, media: [], matches: [], plan: null, step: 1 };

const $ = (selector) => document.querySelector(selector);
const $$ = (selector) => [...document.querySelectorAll(selector)];

function showStep(step) {
  state.step = step;
  $$(".screen").forEach((item) => item.classList.toggle("active", item.id === `screen-${step}`));
  $$(".step").forEach((item) => {
    const number = Number(item.dataset.step);
    item.classList.toggle("active", number === step);
    item.classList.toggle("complete", number < step);
  });
}

function toast(message) {
  const element = $("#toast");
  element.textContent = message;
  element.classList.add("show");
  setTimeout(() => element.classList.remove("show"), 3200);
}

async function api(path, options = {}) {
  const response = await fetch(path, { headers: { "Content-Type": "application/json" }, ...options });
  const payload = await response.json();
  if (!response.ok) throw new Error(payload.error?.message || "Action impossible");
  return payload;
}

function currentSettings() {
  const [width, height] = $("#resolution").value.split("x").map(Number);
  return {
    style_id: $("#style").value,
    delivery: {
      platform: $("#platform").value,
      width,
      height,
      fps: $("#fps").value,
      target_duration_seconds: Number($("#targetDuration").value),
    },
  };
}

async function createProject() {
  try {
    const settings = currentSettings();
    const created = await api("/api/projects", { method: "POST", body: JSON.stringify({ name: $("#projectName").value, ...settings }) });
    state.project = created;
    toast(`Projet prêt : ${created.name}`);
    showStep(2);
  } catch (error) { toast(error.message); }
}

function sceneCard(scene) {
  const narration = scene.narration?.text || "Aucune narration";
  const terms = (scene.visual?.search_terms || []).join(", ") || "Non défini";
  const graphics = (scene.graphics || []).map((item) => `${item.kind} : ${item.text}`).join(" | ") || "Aucun";
  return `<article class="scene-card"><div class="scene-head"><div class="scene-title">${scene.title}</div><span class="pill">${scene.editing.target_seconds}s</span></div><div class="scene-purpose">${scene.purpose}</div><div class="scene-columns"><div class="scene-detail"><strong>Narration</strong>${narration}</div><div class="scene-detail"><strong>Plans recherchés</strong>${terms}</div><div class="scene-detail"><strong>Texte écran</strong>${graphics}</div><div class="scene-detail"><strong>Contraintes</strong>${(scene.constraints?.mandatory_elements || []).join(" · ") || "Aucune"}</div></div></article>`;
}

async function generateStoryboard() {
  try {
    const generated = await api(`/api/projects/${state.project.id}/storyboard`, { method: "POST", body: JSON.stringify({ scenario: $("#scenario").value }) });
    state.storyboard = generated.storyboard;
    $("#storyboardMeta").textContent = `${generated.storyboard.scenes.length} scènes · version ${generated.version}`;
    $("#sceneList").innerHTML = generated.storyboard.scenes.map(sceneCard).join("");
    showStep(3);
  } catch (error) { toast(error.message); }
}

async function chooseStyle() {
  try {
    const style = {
      schema_version: "1.0.0",
      id: "crossroads_classic",
      name: "Crossroads Classic",
      editing: {
        shot_min_seconds: 3,
        shot_max_seconds: 6,
        default_transition: "cut",
        avoid_jump_cuts: true,
      },
      audio: {
        music_profile: "soft_cooking",
        music_level_db: -20,
        duck_level_db: -27,
        keep_natural_sound: true,
      },
      graphics: {
        title_template_id: "crossroads_title_v1",
        ingredients_template_id: "crossroads_ingredients_v2",
        subtitle_template_id: "crossroads_subtitle_v1",
      },
    };
    await api(`/api/projects/${state.project.id}/style`, {
      method: "POST",
      body: JSON.stringify({ style }),
    });
    toast("Style appliqué");
    showStep(5);
  } catch (error) { toast(error.message); }
}

async function analyzeMedia() {
  try {
    const paths = $("#mediaPaths").value.split(/\r?\n/).map((item) => item.trim()).filter(Boolean);
    const result = await api("/api/media/analyze", { method: "POST", body: JSON.stringify({ paths }) });
    state.media = result.items;
    $("#analysisResult").innerHTML = result.items.map((item) => `<div class="asset-row"><strong>${item.analysis.asset_id}</strong><span>${item.analysis.path}</span><span>${item.analysis.kind}</span><span class="score">${item.analysis.duration_seconds || "?"}s</span></div>`).join("");
    toast(`${result.items.length} média(s) analysé(s)`);
  } catch (error) { toast(error.message); }
}

async function matchMedia() {
  try {
    const segments = state.media.map((item) => item.segment);
    const result = await api(`/api/projects/${state.project.id}/match`, { method: "POST", body: JSON.stringify({ segments }) });
    state.matches = result.matches;
    $("#matchList").innerHTML = result.matches.map((item) => `<article class="match-card"><div class="match-head"><div class="match-title">${item.request_id}</div><span class="pill">${item.selected ? "Sélectionné" : "À compléter"}</span></div><div class="scene-purpose">${item.selected?.explanation || "Aucun candidat ne respecte les contraintes."}</div>${item.alternatives.map((candidate) => `<div class="asset-row"><strong>Alternative</strong><span>${candidate.asset_id}</span><span>${candidate.score_breakdown.duration_fit || 0}</span><span class="score">${candidate.score}</span></div>`).join("")}</article>`).join("");
    showStep(6);
  } catch (error) { toast(error.message); }
}

async function compilePlan() {
  try {
    const result = await api(`/api/projects/${state.project.id}/compile`, { method: "POST", body: "{}" });
    state.plan = result.edit_plan;
    $("#planSummary").textContent = `${result.edit_plan.operations.length} opérations portables · version ${result.version}. Les transitions, titres et niveaux restent inspectables avant application.`;
    $("#operationTable").innerHTML = `<div class="operation-row"><span>ID</span><span>Opération</span><span>Scène</span><span>Durée</span></div>${result.edit_plan.operations.map((operation) => `<div class="operation-row"><strong>${operation.id}</strong><span>${operation.kind}</span><span>${operation.source_scene_id || "global"}</span><span>${operation.params.duration_frames || ""}</span></div>`).join("")}`;
    showStep(7);
  } catch (error) { toast(error.message); }
}

function dryRun() {
  $("#resolveResult").textContent = "Le dry run du plan est prêt localement. L'application DaVinci Resolve reste non confirmée tant que le contrôle d'approbation du script n'est pas réparé. Consultez artifacts/poc/davinci-capability-report.json.";
  showStep(8);
}

$("#createProject").addEventListener("click", createProject);
$("#generateStoryboard").addEventListener("click", generateStoryboard);
$("#regenerateStoryboard").addEventListener("click", generateStoryboard);
$("#chooseStyle").addEventListener("click", chooseStyle);
$("#analyzeMedia").addEventListener("click", analyzeMedia);
$("#matchMedia").addEventListener("click", matchMedia);
$("#compilePlan").addEventListener("click", compilePlan);
$("#dryRun").addEventListener("click", dryRun);
$$("[data-go]").forEach((button) => button.addEventListener("click", () => showStep(Number(button.dataset.go))));

