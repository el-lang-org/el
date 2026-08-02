#[derive(Debug)]
struct Registration<'a> {
    username: &'a str,
    email: &'a str,
    age: i64,
    referral: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
struct User<'a> {
    username: &'a str,
    email: &'a str,
    age: i64,
    referral: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
enum ValidationError<'a> {
    InvalidUsernameLength(usize),
    InvalidEmail(&'a str),
    InvalidAge(i64),
    InvalidReferralLength(usize),
}

fn main() {
    for registration in [
        Registration {
            username: "Mina",
            email: "mina@example.com",
            age: 28,
            referral: Some("EL2026"),
        },
        Registration {
            username: "李雷",
            email: "li@example.com",
            age: 31,
            referral: None,
        },
        Registration {
            username: "Nora",
            email: "nora.example.com",
            age: 22,
            referral: None,
        },
        Registration {
            username: "Sam",
            email: "sam@example.com",
            age: 12,
            referral: None,
        },
        Registration {
            username: "Grace",
            email: "grace@example.com",
            age: 37,
            referral: Some("SHORT"),
        },
    ] {
        print_result(registration);
    }
}

fn print_result(registration: Registration<'_>) {
    match validate(registration) {
        Ok(user) => println!("accepted: {}", user.username),
        Err(ValidationError::InvalidUsernameLength(length)) => {
            println!("rejected: username has {length} code points (expected 3..20)")
        }
        Err(ValidationError::InvalidEmail(email)) => {
            println!("rejected: invalid email {email}")
        }
        Err(ValidationError::InvalidAge(age)) => {
            println!("rejected: age {age} is outside 13..120")
        }
        Err(ValidationError::InvalidReferralLength(length)) => {
            println!("rejected: referral has {length} code points (expected 6)")
        }
    }
}

fn validate(registration: Registration<'_>) -> Result<User<'_>, ValidationError<'_>> {
    let username_length = registration.username.chars().count();
    if !(3..=20).contains(&username_length) {
        return Err(ValidationError::InvalidUsernameLength(username_length));
    }
    if !valid_email(registration.email) {
        return Err(ValidationError::InvalidEmail(registration.email));
    }
    if !(13..=120).contains(&registration.age) {
        return Err(ValidationError::InvalidAge(registration.age));
    }
    if let Some(code) = registration.referral {
        let code_length = code.chars().count();
        if code_length != 6 {
            return Err(ValidationError::InvalidReferralLength(code_length));
        }
    }

    Ok(User {
        username: registration.username,
        email: registration.email,
        age: registration.age,
        referral: registration.referral,
    })
}

fn valid_email(email: &str) -> bool {
    let mut parts = email.split('@');
    let Some(local) = parts.next() else {
        return false;
    };
    let Some(domain) = parts.next() else {
        return false;
    };

    !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && parts.next().is_none()
}
