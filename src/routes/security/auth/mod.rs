pub mod login;
pub mod mfa;
pub mod magic_link;
pub mod password_reset;
pub mod setup;
pub mod logout;

pub fn routes() -> Vec<rocket::Route> {
    routes![
        login::login_page,
        login::login_submit,
        mfa::mfa_page,
        mfa::mfa_submit,
        magic_link::magic_link_page,
        magic_link::magic_link_submit,
        magic_link::magic_link_verify,
        password_reset::forgot_password_page,
        password_reset::forgot_password_submit,
        password_reset::reset_password_page,
        password_reset::reset_password_submit,
        logout::logout,
        logout::admin_redirect_to_login,
        setup::setup_page,
        setup::setup_submit,
        setup::test_mongo_connection,
    ]
}
