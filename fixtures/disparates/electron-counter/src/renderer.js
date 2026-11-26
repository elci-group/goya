'use strict';

const amount = document.getElementById('window-amount');
const add = document.getElementById('window-add');
const total = document.getElementById('window-total');

// Copying from the amount field is allowed; pasting into it is not.
amount.addEventListener('paste', (event) => {
  event.preventDefault();
});

// Presses are serialised: a second press waits for the first to finish.
let pending = Promise.resolve();

add.addEventListener('click', () => {
  const text = amount.value;
  pending = pending.then(async () => {
    const result = await window.counter.add(text);
    total.textContent = result.count;
  });
});

window.counter.get().then((count) => {
  total.textContent = count;
});
