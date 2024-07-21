const textElement = document.getElementById("game-text");
const scoreElement = document.getElementById("game-score");
const canvas = document.getElementById("game");
const context = canvas.getContext("2d");

const CELL_SIZE = 16;
const START_Y = 160;
const START_X = 160;

const snake = {
  x: 0,
  y: 0,
  dx: 0,
  dy: 0,
  cells: [],
  maxCells: 0,
};
const apple = {
  x: 0,
  y: 0,
};
let score = 0;

function randomCell() {
  return Math.floor(Math.random() * (canvas.width / CELL_SIZE)) * CELL_SIZE;
}

function reset() {
  snake.x = START_X;
  snake.y = START_Y;
  snake.cells = [];
  snake.maxCells = 4;
  snake.dx = 1;
  snake.dy = 0;

  apple.x = randomCell();
  apple.y = randomCell();
  scoreElement.innerText = 0;
  score = 0;
  setMessage(0);
}

function setMessage(score) {
  let text = "";
  switch (score) {
    case 0:
      text = "Uh oh...";
      break;
    case 5:
      text = "Maybe you should be studying";
      break;
    case 6:
      text = "Or not";
      break;
    case 9:
      text = "Press ESC to leave";
      break;
    case 14:
      text = "Would 王老师 be disappointed??";
      break;
    case 16:
      text = "Or, even worse, 袜子?";
      break;
    case 22:
      text = 'Tell Parker: "the cows are back"';
      break;
    case 25:
      text = "You've accomplished so much!";
      break;
    case 27:
      text = "108 S Sandusky St, Delaware, OH 43015";
      break;
    case 30:
      text = "老师 made a mistake asking me to make this website";
      break;
    case 34:
      text = "You win $100,000,000!";
      break;
    case 39:
      text = "Parker is taller than Kais";
      break;
    case 43:
      text = "How did the chicken cross the road?";
      break;
    case 50:
      text = "Okay you should definitely not be this far";
      break;
    default:
      return;
  }
  textElement.innerText = text;
}

let fpsCounter = 0;

function loop() {
  requestAnimationFrame(loop);

  // 15 fps
  if (++fpsCounter < 4) {
    return;
  }

  fpsCounter = 0;
  context.clearRect(0, 0, canvas.width, canvas.height);

  snake.x += snake.dx * CELL_SIZE;
  snake.y += snake.dy * CELL_SIZE;

  // wrap snake position horizontally on edge of screen
  if (snake.x < 0) {
    reset();
  } else if (snake.x >= canvas.width) {
    reset();
  }
  // wrap snake position vertically on edge of screen
  if (snake.y < 0) {
    reset();
  } else if (snake.y >= canvas.height) {
    reset();
  }

  // keep track of where snake has been. front of the array is always the head
  snake.cells.unshift({ x: snake.x, y: snake.y });

  // remove cells as we move away from them
  if (snake.cells.length > snake.maxCells) {
    snake.cells.pop();
  }

  // Apple fill
  context.fillStyle = "black";
  context.fillRect(apple.x, apple.y, CELL_SIZE - 1, CELL_SIZE - 1);

  // Snake fill
  context.fillStyle = "black";
  snake.cells.forEach((cell, index) => {
    context.fillRect(cell.x, cell.y, CELL_SIZE - 1, CELL_SIZE - 1);

    // Snake ate apple
    if (cell.x === apple.x && cell.y === apple.y) {
      snake.maxCells++;
      scoreElement.innerText = ++score;

      apple.x = randomCell();
      apple.y = randomCell();

      setMessage(score);
    }

    for (let i = index + 1; i < snake.cells.length; i++) {
      // snake occupies same space as a body part. reset game
      if (cell.x === snake.cells[i].x && cell.y === snake.cells[i].y) {
        reset();
      }
    }
  });
}

document.addEventListener("keydown", function (e) {
  if (e.key === "ArrowLeft" && snake.dx === 0) {
    snake.dx = -1;
    snake.dy = 0;
  }
  if (e.key === "ArrowUp" && snake.dy === 0) {
    snake.dy = -1;
    snake.dx = 0;
  }
  if (e.key === "ArrowRight" && snake.dx === 0) {
    snake.dx = 1;
    snake.dy = 0;
  }
  if (e.key === "ArrowDown" && snake.dy === 0) {
    snake.dy = 1;
    snake.dx = 0;
  }
  if (e.key == "Escape") {
    window.location.href = "/";
  }
});

reset();
// Start
requestAnimationFrame(loop);
