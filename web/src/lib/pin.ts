/** PIN ditolak server: kosongkan kotak PIN di form itu, tandai merah, fokus lagi ke kotak pertama. */
export function pinSalah(form: HTMLFormElement) {
  form.querySelectorAll<HTMLInputElement>('.pin-asli').forEach((input, n) => {
    input.value = '';
    input.dispatchEvent(new Event('input'));
    input.closest('.pin-kotak')?.classList.add('salah');
    if (n === 0) input.focus();
  });
}
