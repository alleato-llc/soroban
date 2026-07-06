function jumpTo(id) {
  const el = document.getElementById(id);
  if (!el) return;
  if (el.tagName === 'DETAILS') el.open = true;
  const f = el.closest('details.feature');
  if (f) f.open = true;
  el.classList.remove('hidden');
  el.scrollIntoView({ behavior: 'smooth', block: 'start' });
}
(function () {
  const links = new Map();
  document.querySelectorAll('.rail a[data-target]').forEach(a => links.set(a.dataset.target, a));
  if (!links.size) return;
  let current = null;
  const obs = new IntersectionObserver((entries) => {
    for (const e of entries) { if (e.isIntersecting) current = e.target.id; }
    links.forEach(a => a.removeAttribute('aria-current'));
    const active = current && links.get(current);
    if (active) active.setAttribute('aria-current', 'true');
  }, { rootMargin: '-10% 0px -80% 0px' });
  links.forEach((_, id) => { const el = document.getElementById(id); if (el) obs.observe(el); });
})();
function cycleTheme() {
  const root = document.documentElement;
  const next = root.getAttribute('data-theme') === 'dark' ? 'light' : 'dark';
  root.setAttribute('data-theme', next);
  localStorage.setItem('soroban-theme', next);
}
function toggleRail() {
  const root = document.documentElement;
  if (root.getAttribute('data-rail') === 'collapsed') {
    root.removeAttribute('data-rail');
    localStorage.setItem('soroban-rail', 'expanded');
  } else {
    root.setAttribute('data-rail', 'collapsed');
    localStorage.setItem('soroban-rail', 'collapsed');
  }
}
