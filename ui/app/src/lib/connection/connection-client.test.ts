import { describe, expect, it } from 'vitest';

import {
	describeTestResult,
	validateConnectionForm,
	type ConnectionTestResult
} from './connection-client.ts';

describe('validateConnectionForm', () => {
	it('accepts a well-formed http(s) server', () => {
		expect(validateConnectionForm({ name: 'Home', url: 'https://notes.example.com' })).toBeNull();
		expect(validateConnectionForm({ name: 'Home', url: 'http://127.0.0.1:27183' })).toBeNull();
	});

	it('requires a non-empty name', () => {
		expect(validateConnectionForm({ name: '   ', url: 'https://notes.example.com' })).toMatch(
			/name is required/i
		);
	});

	it('requires a URL', () => {
		expect(validateConnectionForm({ name: 'Home', url: '  ' })).toMatch(/url is required/i);
	});

	it('rejects a malformed URL', () => {
		expect(validateConnectionForm({ name: 'Home', url: 'not a url' })).toMatch(/valid url/i);
	});

	it('rejects non-http(s) schemes', () => {
		expect(validateConnectionForm({ name: 'Home', url: 'ftp://host/x' })).toMatch(
			/http:\/\/ or https:\/\//i
		);
	});
});

describe('describeTestResult', () => {
	it('summarizes a reachable result with latency and vault count', () => {
		const result: ConnectionTestResult = {
			reachable: true,
			latency_ms: 42,
			vault_count: 3
		};
		expect(describeTestResult(result)).toBe('Reachable · 42 ms · 3 vaults');
	});

	it('singularizes a single vault', () => {
		expect(describeTestResult({ reachable: true, latency_ms: 5, vault_count: 1 })).toBe(
			'Reachable · 5 ms · 1 vault'
		);
	});

	it('surfaces the error for an unreachable result', () => {
		expect(describeTestResult({ reachable: false, error: 'Connection refused' })).toBe(
			'Connection refused'
		);
	});

	it('falls back to a generic label when no error is given', () => {
		expect(describeTestResult({ reachable: false })).toBe('Unreachable');
	});
});
