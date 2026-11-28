<?php

declare(strict_types=1);

namespace App\Console;

use App\Domain\Request\CreateOperatorRequest;
use App\Domain\UserRole;
use App\Exception\AlreadyExistsException;
use App\Exception\ValidationException;
use App\Service\UserService;

/**
 * 최초 운영자 계정 생성 — `bin/console operator:create` 로 실행한다.
 *
 * 배포는 마이그레이션만 돌리고 시더를 실행하지 않으므로, 운영 서버에 운영자가 없을 때
 * 이 커맨드로 첫 운영자를 만든다. 이메일 인증은 즉시 완료된다.
 * 같은 소속에 같은 이메일이 이미 있으면 아무것도 바꾸지 않고 건너뛴다(재실행 안전).
 *
 * 비밀번호는 셸 히스토리·프로세스 목록에 남지 않도록 옵션이 아니라 표준입력으로 받는다.
 */
final readonly class CreateOperator
{
    public function __construct(
        private UserService $users,
    ) {
    }

    /**
     * `--key=value` 형식의 옵션만 모은다(그 외 인자는 무시).
     *
     * @param list<string> $argv
     *
     * @return array<string, string>
     */
    public static function parseOptions(array $argv): array
    {
        $options = [];
        foreach ($argv as $arg) {
            if (preg_match('/^--([a-z]+)=(.*)$/', $arg, $matches) === 1) {
                $options[$matches[1]] = $matches[2];
            }
        }

        return $options;
    }

    /**
     * 운영자 계정을 만들고 결과 메시지를 반환한다.
     *
     * @param array<string, string> $options email·name·affiliation·contact(필수), company(선택)
     *
     * @throws ValidationException 옵션·비밀번호 검증 실패
     */
    public function run(array $options, string $password): string
    {
        $missing = array_values(array_filter(
            ['email', 'name', 'affiliation', 'contact'],
            static fn (string $key): bool => ($options[$key] ?? '') === '',
        ));
        if ($missing !== []) {
            throw new ValidationException(array_map(
                static fn (string $key): string => "--{$key} 옵션이 필요합니다.",
                $missing,
            ));
        }

        $request = CreateOperatorRequest::fromArray([
            'email' => $options['email'],
            'password' => $password,
            'role' => UserRole::Operator->value,
            'affiliation' => $options['affiliation'],
            'name' => $options['name'],
            'contact' => $options['contact'],
            'company' => $options['company'] ?? null,
            'profile' => [],
        ]);

        try {
            $id = $this->users->createInitialOperator($request);
        } catch (AlreadyExistsException) {
            return "이미 존재하는 계정입니다({$request->email}, {$request->affiliation->value}) — 건너뜁니다.";
        }

        return "운영자 계정 생성 완료: id={$id}, {$request->email} ({$request->affiliation->value})";
    }
}
