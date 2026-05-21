// tools/load/declaration-get.js
//
// TODO-053 — capacity baseline for the declaration GET hot-path.
//
// Drives 200 RPS for 5 minutes against
// `GET /v1/declarations/{id}` on the dev compose stack. Asserts the
// SLO documented at services/declaration/CLAUDE.md § SLOs:
//
//   GET /v1/declarations/{id}  ⇒  p99 < 50 ms, availability ≥ 99.95%
//
// The GET hot-path is read-only and may be cached aggressively; a
// p99 of 50ms is achievable with a single round-trip to the
// Postgres projection. A regression here is the canary that the
// projection-read path has gained an N+1 query or a missing index.
//
// Doctrines: same as declaration-submit.js. See header there.

import http from 'k6/http';
import { check } from 'k6';
import { Counter, Trend } from 'k6/metrics';

export const options = {
    scenarios: {
        get_hot_path: {
            executor: 'constant-arrival-rate',
            rate: 200,
            timeUnit: '1s',
            duration: '5m',
            preAllocatedVUs: 100,
            maxVUs: 400,
        },
    },
    thresholds: {
        'http_req_duration{name:get}': [
            { threshold: 'p(99)<50', abortOnFail: true },
        ],
        'http_req_failed{name:get}': [
            { threshold: 'rate<0.0005', abortOnFail: true },
        ],
    },
};

const getLatency = new Trend('declaration_get_latency');
const getOk = new Counter('declaration_get_2xx');

const URL = __ENV.RECOR_DECLARATION_URL || 'http://localhost:8081';
const TOKEN = __ENV.RECOR_PRINCIPAL_TOKEN || '';

// SEED_DECLARATION_IDS is a comma-separated list of UUIDs the
// caller has pre-loaded into the declaration database. The
// load-baseline workflow seeds 1k declarations in setup; this var
// is the resulting id list.
const ID_LIST = (__ENV.SEED_DECLARATION_IDS || '').split(',').filter(Boolean);
if (ID_LIST.length === 0) {
    throw new Error('SEED_DECLARATION_IDS must contain at least one UUID');
}

export default function () {
    const id = ID_LIST[Math.floor(Math.random() * ID_LIST.length)];
    const params = {
        headers: {
            'Authorization': `Bearer ${TOKEN}`,
        },
        tags: { name: 'get' },
        timeout: '2s',
    };
    const res = http.get(`${URL}/v1/declarations/${id}`, params);
    getLatency.add(res.timings.duration);
    check(res, {
        '2xx response': r => r.status === 200,
        'declaration_id matches': r => {
            try { return JSON.parse(r.body).declaration_id === id; }
            catch { return false; }
        },
    }) && getOk.add(1);
}

export function handleSummary(data) {
    return {
        'stdout': textSummary(data),
        'load-report.html': htmlReport(data),
    };
}

import { textSummary } from 'https://jslib.k6.io/k6-summary/0.0.2/index.js';
import { htmlReport } from 'https://raw.githubusercontent.com/benc-uk/k6-reporter/2.4.0/dist/bundle.js';
