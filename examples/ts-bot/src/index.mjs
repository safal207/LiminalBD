import Fastify from 'fastify';
import { Telegraf } from 'telegraf';
import { Client } from '@liminaldb/client';

const fastify = Fastify({ logger: true });
const client = new Client({
  url: process.env.LIMINALDB_URL ?? 'wss://demo.liminaldb.dev/ws',
  keyId: process.env.LIMINALDB_KEY_ID,
  secret: process.env.LIMINALDB_SECRET,
  reconnect: true
});

client.on('status', (event) => {
  fastify.log.info({ status: event }, 'status event');
});

client.connect();

fastify.get('/healthz', async () => ({ ok: true }));

const botToken = process.env.BOT_TOKEN;
if (botToken) {
  const bot = new Telegraf(botToken);
  bot.command('status', (ctx) => ctx.reply('LiminalDB bot online.'));
  bot.command('explain', (ctx) => {
    const [, token] = ctx.message.text.split(' ');
    ctx.reply(`Explain token ${token ?? 'missing'}`);
  });
  bot.launch();
  fastify.addHook('onClose', async () => bot.stop('fastify shutdown'));
}

fastify.listen({ port: Number(process.env.PORT ?? 3000), host: '0.0.0.0' });
