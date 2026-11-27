<?php

declare(strict_types=1);

namespace App\Controller;

use App\Support\Config;
use App\Support\Version;
use Psr\Http\Message\ResponseFactoryInterface;
use Psr\Http\Message\ResponseInterface;
use Psr\Http\Message\ServerRequestInterface;
use Psr\Http\Message\StreamFactoryInterface;

/**
 * 검색엔진·AI 크롤러용 공개 파일 — robots.txt, sitemap.xml, llms.txt.
 *
 * 절대 URL 은 APP_BASE_URL 에서 만든다. 소개 페이지(`/`)만 색인 대상으로 알리고
 * `/api/` 하위(인증·회원 API)는 크롤링 대상에서 제외한다. 민감 정보는 담지 않는다.
 */
final readonly class SeoController
{
    /** 크롤링을 허용할 AI 크롤러 User-Agent (소개 페이지 학습·인용 허용). */
    private const array AI_CRAWLERS = ['GPTBot', 'ChatGPT-User', 'OAI-SearchBot', 'ClaudeBot', 'Claude-User', 'PerplexityBot', 'Google-Extended'];

    public function __construct(
        private Config $config,
        private ResponseFactoryInterface $responseFactory,
        private StreamFactoryInterface $streamFactory,
    ) {
    }

    public function robots(ServerRequestInterface $request): ResponseInterface
    {
        $rules = "Allow: /\nDisallow: /api/\n";
        $body = "User-agent: *\n" . $rules;
        foreach (self::AI_CRAWLERS as $agent) {
            $body .= "\nUser-agent: {$agent}\n" . $rules;
        }
        $body .= "\nSitemap: {$this->baseUrl()}/sitemap.xml\n";

        return $this->text($body, 'text/plain');
    }

    public function sitemap(ServerRequestInterface $request): ResponseInterface
    {
        $loc = htmlspecialchars($this->baseUrl() . '/', ENT_XML1 | ENT_QUOTES);
        $body = <<<XML
        <?xml version="1.0" encoding="UTF-8"?>
        <urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
          <url>
            <loc>{$loc}</loc>
            <changefreq>monthly</changefreq>
            <priority>1.0</priority>
          </url>
        </urlset>

        XML;

        return $this->text($body, 'application/xml');
    }

    public function llms(ServerRequestInterface $request): ResponseInterface
    {
        $base = $this->baseUrl();
        $name = Version::APP_NAME;
        $body = <<<MD
        # {$name}

        > {$name} 는 AIvance 제품군을 위한 JWT 기반 인증·인가 REST API 입니다. 가입, 이메일 인증, 로그인, 토큰 갱신, 로그아웃, 회원 관리를 하나의 API 로 제공합니다.

        - 토큰: Access 토큰 15분, Refresh 토큰 14일(회전·폐기 지원). 서명 알고리즘은 HS256 또는 RS256 으로 고정합니다.
        - 역할: 운영자, 대행사, 일반회원 (JWT role 클레임 기반 접근 제어).
        - 공개키: RS256 사용 시 JWKS(RFC 7517)로 공개키를 제공합니다.
        - 보호된 API 는 Authorization: Bearer 토큰이 필요합니다.

        ## Docs

        - [소개 페이지]({$base}/): 기능, 아키텍처, 적용 기술, FAQ
        - [API 문서 (Swagger)]({$base}/api/docs): 요청·응답 스키마
        - [OpenAPI 스펙]({$base}/api/v1/openapi.json): 기계 판독용 API 명세
        - [JWKS]({$base}/.well-known/jwks.json): 토큰 검증용 공개키 세트

        MD;

        return $this->text($body, 'text/plain');
    }

    private function baseUrl(): string
    {
        return rtrim($this->config->appBaseUrl, '/');
    }

    private function text(string $body, string $mime): ResponseInterface
    {
        return $this->responseFactory
            ->createResponse(200)
            ->withHeader('Content-Type', $mime . '; charset=utf-8')
            ->withHeader('Cache-Control', 'public, max-age=3600')
            ->withBody($this->streamFactory->createStream($body));
    }
}
