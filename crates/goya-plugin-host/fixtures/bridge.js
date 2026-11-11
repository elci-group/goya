window.addEventListener('message', event => {
  if (event.origin !== location.origin) return;
  document.querySelector('#bridge-result').textContent = JSON.stringify(event.data);
});
window.requestGoyaSummary = () => {
  const scope = window.__goyaBridge;
  window.goyaHostRequest(JSON.stringify({requestId:'smoke-1',instanceId:scope.instanceId,
    sessionId:scope.sessionId,generation:scope.generation,operation:'session.readSummary'}));
};
