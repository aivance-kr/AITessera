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
 * 소개 페이지 — 루트(`/`) 접속 시 AITessera 를 설명하는 정적 HTML 을 반환한다.
 *
 * 인증 없이 열람 가능한 공개 페이지이며, 표준 JSON 응답 래퍼를 쓰지 않으므로
 * BaseController 를 상속하지 않는다. 본문은 `resources/landing.html` 템플릿이다.
 */
final readonly class LandingController
{
    private const string TEMPLATE_PATH = __DIR__ . '/../../resources/landing.html';

    public function __construct(
        private Config $config,
        private ResponseFactoryInterface $responseFactory,
        private StreamFactoryInterface $streamFactory,
    ) {
    }

    public function index(ServerRequestInterface $request): ResponseInterface
    {
        $template = (string) file_get_contents(self::TEMPLATE_PATH);
        // 절대 URL(canonical·OG·JSON-LD)은 APP_BASE_URL 에서 만든다 — 환경(로컬/운영)별로 달라진다.
        $html = str_replace(
            ['{{version}}', '{{base_url}}'],
            [
                htmlspecialchars(Version::APP_VERSION, ENT_QUOTES),
                htmlspecialchars(rtrim($this->config->appBaseUrl, '/'), ENT_QUOTES),
            ],
            $template,
        );

        return $this->responseFactory
            ->createResponse(200)
            ->withHeader('Content-Type', 'text/html; charset=utf-8')
            ->withHeader('Cache-Control', 'public, max-age=300')
            ->withBody($this->streamFactory->createStream($html));
    }
}
