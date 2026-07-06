(function () {
  var t = localStorage.getItem('soroban-theme');
  if (!t) t = matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  document.documentElement.setAttribute('data-theme', t);
  if (localStorage.getItem('soroban-rail') === 'collapsed')
    document.documentElement.setAttribute('data-rail', 'collapsed');
})();
