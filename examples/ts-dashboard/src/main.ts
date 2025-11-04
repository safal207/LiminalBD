import { Client } from '@liminaldb/client';

const metricsEl = document.createElement('pre');
metricsEl.textContent = 'Connecting to LiminalDB...';
document.body.appendChild(metricsEl);

const harmonyEl = document.createElement('pre');
harmonyEl.textContent = 'Awaiting harmony stream...';
document.body.appendChild(harmonyEl);

const client = new Client({
  url: 'wss://demo.liminaldb.dev/ws',
  reconnect: true,
  backoff: { min: 250, max: 5000, factor: 2 },
  qpsLimit: 40,
  telemetry: (metrics) => {
    metricsEl.textContent = `state=${metrics.state}\nqueue=${metrics.queueDepth}\nsent=${metrics.sentCommands}\nreceived=${metrics.receivedEvents}`;
  }
});

client.on('harmony', (event) => {
  harmonyEl.textContent = JSON.stringify(event, null, 2);
});

client.connect();
const subscriptionId = client.subscribe('harmony/*');
metricsEl.textContent += `\nSubscription ${subscriptionId}`;
