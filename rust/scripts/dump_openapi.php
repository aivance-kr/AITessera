<?php

declare(strict_types=1);

// PHP 컨트롤러의 OpenAPI 어트리뷰트로부터 스펙을 생성해 stdout 으로 출력한다.
// Rust 서버는 이 스냅샷(rust/static/openapi.json)을 컴파일 시 내장해 그대로 서빙한다.
// 사용법(저장소 루트): php rust/scripts/dump_openapi.php > rust/static/openapi.json

$root = dirname(__DIR__, 2);
require $root . '/vendor/autoload.php';

$openapi = \OpenApi\Generator::scan([$root . '/src']);
if ($openapi === null) {
    fwrite(STDERR, "OpenAPI 스펙 생성에 실패했습니다.\n");
    exit(1);
}
echo $openapi->toJson();
